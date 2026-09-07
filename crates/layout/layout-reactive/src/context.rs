//! The layout runtime the widget tree drives: node rects as signals, dirty roots, and the overlay host.

use geometry_core::Rect;
use layout_core::{AvailableSpace, LayoutEngine, LayoutError, LayoutStyle, MeasureFn, NodeId};
use reactive_core::{RwSignal, batch, signal};
use rustc_hash::FxHashMap;

reactive_core::surface_local! {
    /// Which node each node hangs from — the tree's *shape*, kept in its own world rather than inside [`LAYOUT_RUNTIME`].
    ///
    /// It sits apart because of who has to read it and when: a measure closure runs *inside* the layout runtime's own borrow, and anything it reaches for that lives in that borrow is a re-entry. Text measured against a style its ancestors declared is exactly that reach, and the links it needs are structure, not layout state — nothing in a layout pass moves them.
    slot PARENTS: FxHashMap<NodeId, NodeId> = FxHashMap::default();
    access with_parents, with_parents_ref;
    context ParentsContext, ParentsGuard;
}

reactive_core::surface_local! {
    /// A per-surface layout tree: the taffy engine plus the node→rect-signal registry. The layout tree is a per-surface world so nodes can be created and laid out from anywhere — including reactive effects (reactive lists) that fire from an effect body. Under M3 several surfaces share one UI thread, so the runner activates each surface's [`LayoutContext`] around its build/event/frame; app code just calls the free functions, which operate on whichever surface is currently active.
    slot LAYOUT_RUNTIME: LayoutRuntime = LayoutRuntime::new();
    access with_runtime, with_runtime_ref;
    context LayoutContext, LayoutGuard;
}

/// Resets the active surface's layout runtime to a fresh, empty tree. The single-window app/preview harness calls this at construction; a multi-surface runner instead gives each surface its own [`LayoutContext`].
pub fn reset_layout_runtime() {
    with_runtime(|rt| *rt = LayoutRuntime::new());
    with_parents(|p| p.clear());
}

/// Creates a leaf node and returns it with the signal its laid-out rect is published through.
pub fn new_leaf(style: LayoutStyle) -> Result<(NodeId, RwSignal<Rect>), LayoutError> {
    with_runtime(|rt| rt.new_leaf(style))
}

/// A leaf whose intrinsic size is computed by `measure` at layout time (e.g. text whose height depends on how many lines it wraps into at the resolved width).
///
/// The closure re-enters the owner that registered it, which is the third place this has to happen and the least obvious: a measure runs deep inside taffy's recursion, long after the build, with no owner stack at all. Text sized from something ambient — the region it sits in, a value its component provided — would otherwise resolve against the surface root and read whatever happened to be there. The effect flush and the event dispatch do the same thing for the same reason.
pub fn new_measured_leaf(
    style: LayoutStyle,
    mut measure: MeasureFn,
) -> Result<(NodeId, RwSignal<Rect>), LayoutError> {
    let owner = reactive_core::current_owner();
    let measure: MeasureFn =
        Box::new(move |input| reactive_core::with_owner(owner, || measure(input)));
    with_runtime(|rt| rt.new_measured_leaf(style, measure))
}

/// Creates a container node holding `children`.
pub fn new_container(style: LayoutStyle, children: &[NodeId]) -> Result<NodeId, LayoutError> {
    with_runtime(|rt| rt.new_container(style, children))
}

/// Lays out `root` against the given space and reflects the result into the signals that watch it. Collects the updates while holding the runtime borrow, then applies them in a batch *after* releasing it — a `.set()` can flush effects, and one of those may itself touch the layout runtime (a reactive list), which would re-enter the borrow.
pub fn compute_layout(
    root: NodeId,
    width: AvailableSpace,
    height: AvailableSpace,
) -> Result<(), LayoutError> {
    // Reconciled here rather than in `set_direction`, so the flip reaches every surface on the thread.
    let direction = crate::direction::current_direction();
    with_runtime(|rt| rt.engine.set_direction(direction));
    let updates = with_runtime(|rt| rt.compute_layout(root, width, height))?;
    batch(|| {
        for update in updates {
            match update {
                Update::Rect(sig, rect) => {
                    if sig.peek() != rect {
                        sig.set(rect);
                    }
                }
                Update::Position(sig, at) => sig.set(at),
            }
        }
    });
    Ok(())
}

/// Re-lays out every root that has been computed at least once, picking up any nodes a reactive change dirtied since the last frame. Each `compute_layout` early-returns when its root is clean and the space is unchanged, so this is cheap on a still frame. The runtime calls it once per redraw (after flushing reactive effects, before rendering) so a data change deep in the tree — e.g. a reactive list adding an item — is reflected in layout without the app shell knowing about it. Node dirtiness propagates up to the root through taffy, so a dirtied list container makes its root recompute.
pub fn relayout_if_dirty() {
    let roots: Vec<(NodeId, AvailableSpace, AvailableSpace)> = with_runtime(|rt| {
        rt.last_space
            .iter()
            .map(|(&n, &(w, h))| (n, w, h))
            .collect()
    });
    for (root, width, height) in roots {
        let _ = compute_layout(root, width, height);
    }
}

/// One signal a completed layout has to move, collected under the runtime borrow and applied outside it.
enum Update {
    Rect(RwSignal<Rect>, Rect),
    /// Window-absolute top-left, for the nodes something anchors to. See `LayoutRuntime::abs_pos_signals`.
    Position(RwSignal<(f32, f32)>, (f32, f32)),
}

/// The signal a node's laid-out rect is published through, or `None` if the node is gone.
pub fn track_layout(node: NodeId) -> Option<RwSignal<Rect>> {
    with_runtime(|rt| rt.track_layout(node))
}

/// The node's WINDOW-absolute rect (top-left from the top-level walk, size from its layout), or `None` if it has not been laid out under a window root yet. Unlike `track_layout`, this is correct even for a node in a sub-root computed separately (whose rect signal is root-local) — use it to anchor a portaled overlay to a trigger, since the portal hoists out of ancestor transforms and needs absolute coordinates.
///
/// This is the trigger's *laid-out* position. Scrolling moves content by a render transform, not by relaying it out, so a node inside a scrolled viewport appears somewhere else on screen — `ui_core::visible_rect` applies the offsets on top of this, which is what an anchored overlay wants.
pub fn absolute_rect(node: NodeId) -> Option<Rect> {
    // Both halves subscribe, which is what makes an anchored overlay follow its trigger on its own.
    let (position, size) = with_runtime(|rt| {
        let position = *rt.abs_pos.get(&node)?;
        let signal = *rt
            .abs_pos_signals
            .entry(node)
            .or_insert_with(|| reactive_core::detached(|| reactive_core::signal(position)));
        Some((signal, rt.registry.get(&node).copied()))
    })?;
    let (x, y) = position.get();
    let size = size.map(|s| s.get()).unwrap_or_default();
    Some(Rect::new(x, y, size.width, size.height))
}

/// The node `node` hangs from, or `None` at a root.
///
/// The **layout-tree** parent, which is not always the visual one: a portalled overlay is recorded against the host it attached to, so anything walking up from inside one arrives where the markup put it rather than where the compositor draws it. That is the link a cascade has to follow — CSS inherits through the document, not through the stacking context — and it is the same one [`is_hidden`] already climbs.
pub fn parent(node: NodeId) -> Option<NodeId> {
    with_parents_ref(|p| p.get(&node).copied())
}

/// `node` and everything above it, nearest first — and it ends even when the links form a cycle.
///
/// Parent links are a map the runtime maintains, not a tree it owns, so a loop in them is reachable: an overlay attached under its own content closes one. Every climb then spins instead of answering, which costs the whole surface rather than the one node that is wrong — resolving a text's inherited style walks this on every build. Tortoise-and-hare ends the walk at the repeat, allocating nothing and costing one extra lookup per two steps, which is what lets the hot path use it.
pub fn ancestors(node: NodeId) -> Ancestors {
    Ancestors {
        cursor: Some(node),
        trail: node,
        halve: false,
    }
}

/// The iterator [`ancestors`] returns.
pub struct Ancestors {
    cursor: Option<NodeId>,
    /// Trails `cursor` at half its speed, so the two meet only inside a cycle.
    trail: NodeId,
    halve: bool,
}

impl Iterator for Ancestors {
    type Item = NodeId;

    fn next(&mut self) -> Option<NodeId> {
        let current = self.cursor?;
        let next = parent(current);
        if self.halve {
            self.trail = parent(self.trail).unwrap_or(self.trail);
        }
        self.halve = !self.halve;
        self.cursor = match next {
            Some(node) if node == self.trail => None,
            next => next,
        };
        Some(current)
    }
}

/// Records that `child` hangs from `parent`.
///
/// The one place a parent link is written, because the invariant every climb here depends on — that following these links from any node reaches a root — belongs to no single caller. A link that would close a cycle is refused rather than stored.
fn link_parent(child: NodeId, parent: NodeId) {
    if ancestors(parent).any(|above| above == child) {
        return;
    }
    with_parents(|p| p.insert(child, parent));
}

/// Whether `node` is `ancestor` or sits anywhere beneath it. Follows the parent links the runtime records, so it crosses into a separately-computed sub-root (a scroll's content) the way the layout tree does.
pub fn is_descendant_of(node: NodeId, ancestor: NodeId) -> bool {
    with_runtime(|rt| rt.is_in_subtree(node, ancestor))
}

/// Whether `node` is out of layout flow, by its own `display:none` or any ancestor's.
///
/// The climb is the point: taffy stops laying out under a hidden node, so a descendant keeps the size it had when it was last shown and looks perfectly ordinary to anyone reading its rect. Only the chain says it is gone. Follows the same parent links as [`is_descendant_of`], so it crosses into a portaled sub-root.
pub fn is_hidden(node: NodeId) -> bool {
    with_runtime(|rt| rt.is_hidden_by_display(node))
}

/// Marks a node's layout stale, so the next compute re-runs the root it belongs to.
pub fn mark_dirty(node: NodeId) -> Result<(), LayoutError> {
    with_runtime(|rt| rt.engine.mark_dirty(node))
}

/// Replaces `node`'s layout style and dirties it, so the next pass lays it out again.
///
/// What a widget whose style is *derived* from reactive state calls when that state moves — a theme's metric tokens, today. Unlike rebuilding the widget it keeps the node, its children, and everything they hold; the engine re-resolves the new style against the current direction exactly as it would at construction.
pub fn set_layout_style(node: NodeId, style: LayoutStyle) -> Result<(), LayoutError> {
    with_runtime(|rt| {
        rt.engine.set_style(node, style)?;
        rt.engine.mark_dirty(node)
    })
}

/// Shows or hides a node in layout flow. A hidden node takes no space (and lays out none of its subtree); mark an ancestor dirty and recompute for the change to take effect. Used for responsive layouts (e.g. collapsing a sidebar on narrow windows).
pub fn set_display(node: NodeId, visible: bool) {
    with_runtime(|rt| rt.engine.set_display(node, visible))
}

/// Lays `node`'s children along the horizontal axis, after the node was built as a column. A reconciling list boxed inside a `row` calls this: its own node exists before it is attached, so the direction it should have cannot be known at construction.
pub fn set_container_row(node: NodeId) {
    with_runtime(|rt| rt.engine.make_flex_row(node));
    mark_dirty(node).ok();
}

/// Whether `node` is a flex row (main axis horizontal). A transparent `for … gap:N` fragment reads its host container's axis to know which edge the per-item gap margin sits on.
pub fn container_is_row(node: NodeId) -> bool {
    with_runtime(|rt| rt.engine.is_row(node))
}

/// Sets `node`'s leading main-axis margin (`left` for a row host, `top` for a column) to `px` — the primitive a transparent `for … gap:N` uses to space its items without a container of its own. Marks the node dirty.
pub fn set_leading_margin(node: NodeId, is_row: bool, px: f32) {
    with_runtime(|rt| rt.engine.set_leading_margin(node, is_row, px))
}

/// Sets `node`'s minimum height to `px` after the initial layout (dirtying it, which propagates up), so a content-measured leaf grows to at least `px` even when its content is shorter. A scrolling editor uses it to fill its viewport so a click anywhere in the empty area — not just over the text — lands on the leaf.
pub fn set_min_height(node: NodeId, px: f32) {
    with_runtime(|rt| rt.engine.set_min_height(node, Some(px)))
}

/// Replaces `parent`'s children with `children`, in order, marking `parent` dirty. Operates on the thread-local runtime; `parent` must be a container already registered in the runtime.
pub fn set_children(parent: NodeId, children: &[NodeId]) -> Result<(), LayoutError> {
    with_runtime(|rt| rt.set_children(parent, children))
}

/// Detaches and frees `node` (a former list item) from the runtime: removes it from the layout tree and drops its rect signal and bookkeeping. The caller must have removed it from its parent's child list (via [`set_children`]) first.
pub fn remove_node(node: NodeId) {
    with_runtime(|rt| rt.remove_node(node))
}

/// Pins the overlay host to `node` — the app's window-spanning root — so overlays always fill the viewport even when the app computes several independent layout roots (e.g. a shell with a separate sidebar root computed after the main one, which the auto-detection would otherwise pick as the host). Call it each relayout with the current main root (it survives hot-reload rebuilds, which mint a new root node). Once pinned, auto-detection no longer overrides the host. The area an overlay may occupy: the laid-out rect of the host its content is attached to, which is the window (or the surface) it will be composed into.
///
/// What a panel needs to stay on screen. Without it an anchored bubble is placed from its trigger alone and runs off whichever edge the trigger happens to be near — which is not a rare case but the common one, a tooltip on the rightmost button of a toolbar.
pub fn overlay_viewport() -> Option<geometry_core::Rect> {
    with_runtime(|rt| {
        let host = rt.overlay_host.node()?;
        rt.engine.layout(host).ok()
    })
}

/// Pins the node overlays attach their content to, overriding the auto-detected one.
pub fn set_overlay_host(node: NodeId) {
    with_runtime(|rt| {
        rt.overlay_host.pin(node);
    });
}

/// Attaches `node` (an overlay's out-of-flow content) as an extra child of the current layout host — the top-level root computed against the window — so it fills the viewport regardless of where the `overlay` was declared in the tree. Returns `true` when attached; `false` when no host has been computed yet (the caller then falls back to normal in-tree layout). The host is marked dirty so the next frame lays the portal out.
pub fn attach_overlay(node: NodeId) -> bool {
    with_runtime(|rt| {
        let Some(host) = rt.overlay_host.node() else {
            return false;
        };
        // A host sitting inside the content it would carry closes a parent cycle, and no root is reachable from it.
        if ancestors(host).any(|above| above == node) {
            return false;
        }
        if rt.engine.add_child(host, node).is_err() {
            return false;
        }
        link_parent(node, host);
        rt.engine.mark_dirty(host).ok();
        true
    })
}

/// Detaches an overlay's content from the layout host (inverse of [`attach_overlay`]); the caller frees it afterwards with [`remove_node`]. A no-op if the host is gone.
pub fn detach_overlay(node: NodeId) {
    with_runtime(|rt| {
        // The host the overlay actually attached to, recorded at attach — not the current one: auto-detection may have moved the host to another root since, and taffy panics if `node` is not a child of what it is removed from.
        if let Some(host) = with_parents(|p| p.remove(&node)) {
            rt.engine.remove_child(host, node).ok();
            rt.engine.mark_dirty(host).ok();
        }
    });
}

/// The layout root overlays attach their out-of-flow content to, so a portal fills the viewport regardless of where it is declared.
///
/// One type rather than a node beside a flag, because the two are one rule: who the host is, and when auto-detection is still allowed to say. Split apart, the rule lived in [`Self::offer`]'s caller and nothing tied it to the flag it reads.
#[derive(Default)]
struct OverlayHost {
    node: Option<NodeId>,
    /// Pinned by the app, so auto-detection must not override it. An app with several independent roots needs this: the window-spanning root is the host, not whichever root happened to be computed last.
    pinned: bool,
}

impl OverlayHost {
    /// The node overlays attach to, or `None` before any root has been computed.
    fn node(&self) -> Option<NodeId> {
        self.node
    }

    /// Pins the host to `node`, so auto-detection no longer overrides it.
    fn pin(&mut self, node: NodeId) {
        self.node = Some(node);
        self.pinned = true;
    }

    /// Offers `root` as the host for the compute about to run, taking it only when the rule allows.
    ///
    /// A top-level root computed against the window is the overlay host, refreshed each compute so it stays current across a hot-reload rebuild. A definite height marks the surface root; a detached sub-root laid out for its intrinsic height must not become the host, or a portal declared inside it would attach to that scroll and be torn down with it.
    ///
    /// `is_top_level` is a closure so the parent map is only borrowed once the cheaper terms have already agreed.
    fn offer(&mut self, root: NodeId, height: AvailableSpace, is_top_level: impl FnOnce() -> bool) {
        if !self.pinned && matches!(height, AvailableSpace::Definite(_)) && is_top_level() {
            self.node = Some(root);
        }
    }
}

struct LayoutRuntime {
    engine: LayoutEngine,
    registry: FxHashMap<NodeId, RwSignal<Rect>>,
    boundary_nodes: FxHashMap<NodeId, (f32, f32)>,
    // So `compute_layout` can re-run when only the space changed.
    last_space: FxHashMap<NodeId, (AvailableSpace, AvailableSpace)>,
    // Nodes with a definite `max-width`, their original style, and the width pinned on the previous compute. Captured the first time each compute-root is computed.
    root_auto: FxHashMap<NodeId, (bool, bool)>,
    overlay_host: OverlayHost,
    // Captured during the top-level root's walk, which runs from the window origin. Node rect signals stay root-local, so this map is the one place with window-absolute positions — which is what lets `absolute_rect` anchor a portaled overlay to a trigger in a sub-root.
    abs_pos: FxHashMap<NodeId, (f32, f32)>,
    /// The observable half of `abs_pos`, minted on first `absolute_rect` and never eagerly.
    ///
    /// Lazy is a constraint, not a preference: `new_leaf` already creates a rect signal per node, so minting a second one per node would double the reactive arena and its subscription bookkeeping for a value almost nothing reads. Absolute position is what a portalled overlay anchors to — a handful of nodes.
    abs_pos_signals: FxHashMap<NodeId, RwSignal<(f32, f32)>>,
    // Guards against a recursive `compute()`: an effect that reads a layout signal and calls `compute_layout`.
    #[cfg(debug_assertions)]
    is_computing: bool,
}

impl LayoutRuntime {
    fn new() -> Self {
        Self {
            engine: LayoutEngine::new(),
            registry: FxHashMap::default(),
            boundary_nodes: FxHashMap::default(),
            last_space: FxHashMap::default(),
            root_auto: FxHashMap::default(),
            overlay_host: OverlayHost::default(),
            abs_pos: FxHashMap::default(),
            abs_pos_signals: FxHashMap::default(),
            #[cfg(debug_assertions)]
            is_computing: false,
        }
    }

    pub(crate) fn new_leaf(
        &mut self,
        style: LayoutStyle,
    ) -> Result<(NodeId, RwSignal<Rect>), LayoutError> {
        let node = self.engine.new_leaf(style.clone())?;
        let signal = signal(Rect::default());
        self.registry.insert(node, signal);
        if let Some(dimensions) = self.engine.is_fixed_size(node) {
            self.boundary_nodes.insert(node, dimensions);
        }
        Ok((node, signal))
    }

    pub(crate) fn new_measured_leaf(
        &mut self,
        style: LayoutStyle,
        measure: MeasureFn,
    ) -> Result<(NodeId, RwSignal<Rect>), LayoutError> {
        let node = self.engine.new_measured_leaf(style.clone(), measure)?;
        let signal = signal(Rect::default());
        self.registry.insert(node, signal);
        Ok((node, signal))
    }

    pub(crate) fn new_container(
        &mut self,
        style: LayoutStyle,
        children: &[NodeId],
    ) -> Result<NodeId, LayoutError> {
        let node = self.engine.new_container(style.clone(), children)?;
        let signal = signal(Rect::default());
        self.registry.insert(node, signal);
        for &child in children {
            link_parent(child, node);
        }
        if let Some(dimensions) = self.engine.is_fixed_size(node) {
            self.boundary_nodes.insert(node, dimensions);
        }
        Ok(node)
    }

    fn compute_layout(
        &mut self,
        root: NodeId,
        width: AvailableSpace,
        height: AvailableSpace,
    ) -> Result<Vec<Update>, LayoutError> {
        self.overlay_host.offer(root, height, || {
            with_parents_ref(|parents| !parents.contains_key(&root))
        });
        // A changed available space must re-run layout even when the node is clean.
        let is_space_changed = self.last_space.get(&root) != Some(&(width, height));
        if is_space_changed {
            self.engine.mark_dirty(root).ok();
            self.last_space.insert(root, (width, height));
        } else if !self.engine.is_dirty(root) {
            return Ok(Vec::new());
        }
        // A layout root fills the definite space it is computed in, so an auto width or height becomes that space.
        let (width_auto, height_auto) = match self.root_auto.get(&root).copied() {
            Some(v) => v,
            None => {
                let v = self.engine.is_size_auto(root);
                self.root_auto.insert(root, v);
                v
            }
        };
        let mut did_fill_root = false;
        if width_auto {
            let w = match width {
                AvailableSpace::Definite(w) => Some(w),
                _ => None,
            };
            self.engine.set_width(root, w);
            did_fill_root = true;
        }
        if height_auto {
            let h = match height {
                AvailableSpace::Definite(h) => Some(h),
                _ => None,
            };
            self.engine.set_height(root, h);
            did_fill_root = true;
        }
        if did_fill_root {
            self.engine.mark_dirty(root).ok();
        }
        let mut dirty_nodes = Vec::new();
        self.engine.collect_dirty_nodes(root, &mut dirty_nodes);
        if dirty_nodes.is_empty() {
            return Ok(Vec::new());
        }
        #[cfg(debug_assertions)]
        {
            assert!(
                !self.is_computing,
                "[rsx layout] cycle detected: compute_layout() called recursively. \
                 An effect is reading a layout signal and then calling compute_layout() again inside its body. \
                 This causes an infinite re-layout loop (capped by MAX_FLUSH_ITERATIONS). \
                 Move style mutations outside of layout-observing effects."
            );
            self.is_computing = true;
        }
        let (layout_root, layout_width, layout_height) =
            self.find_boundary_root(&dirty_nodes, root, width, height);
        self.engine
            .compute_layout(layout_root, layout_width, layout_height)?;
        // Collected under the runtime borrow but applied only after the caller releases it: a `set` flushes effects, one of which may re-enter the runtime.
        let mut updates: Vec<Update> = Vec::new();
        // Only a full walk of a parent-less root runs from the window origin, so only then are the walked rects window-absolute.
        let is_window_walk = layout_root == root && with_parents_ref(|p| !p.contains_key(&root));
        let mut abs_updates: Vec<(NodeId, f32, f32)> = Vec::new();
        let registry = &self.registry;
        let walk_result = self.engine.walk(layout_root, &mut |node_id, rect| {
            if let Some(sig) = registry.get(&node_id) {
                if sig.peek() != rect {
                    updates.push(Update::Rect(*sig, rect));
                }
            }
            if is_window_walk {
                abs_updates.push((node_id, rect.x, rect.y));
            }
            true
        });
        for (n, x, y) in abs_updates {
            self.abs_pos.insert(n, (x, y));
            // Only where somebody asked: a node nobody anchors to has no signal and never gets one.
            if let Some(sig) = self.abs_pos_signals.get(&n)
                && sig.peek() != (x, y)
            {
                updates.push(Update::Position(*sig, (x, y)));
            }
        }
        #[cfg(debug_assertions)]
        {
            self.is_computing = false;
        }
        walk_result.map(|()| updates)
    }

    fn find_boundary_root(
        &self,
        dirty_nodes: &[NodeId],
        global_root: NodeId,
        global_width: AvailableSpace,
        global_height: AvailableSpace,
    ) -> (NodeId, AvailableSpace, AvailableSpace) {
        let candidate = dirty_nodes
            .iter()
            .find_map(|&node| self.find_nearest_boundary(node));
        match candidate {
            Some((boundary, boundary_width, boundary_height))
                if dirty_nodes.iter().all(|&n| self.is_in_subtree(n, boundary)) =>
            {
                (
                    boundary,
                    AvailableSpace::Definite(boundary_width),
                    AvailableSpace::Definite(boundary_height),
                )
            }
            _ => (global_root, global_width, global_height),
        }
    }

    fn find_nearest_boundary(&self, node: NodeId) -> Option<(NodeId, f32, f32)> {
        ancestors(node).find_map(|at| self.boundary_nodes.get(&at).map(|&(w, h)| (at, w, h)))
    }

    fn is_hidden_by_display(&self, node: NodeId) -> bool {
        ancestors(node).any(|at| self.engine.is_display_none(at))
    }

    fn is_in_subtree(&self, node: NodeId, ancestor: NodeId) -> bool {
        ancestors(node).any(|at| at == ancestor)
    }

    pub(crate) fn track_layout(&self, node: NodeId) -> Option<RwSignal<Rect>> {
        self.registry.get(&node).cloned()
    }

    fn set_children(&mut self, parent: NodeId, children: &[NodeId]) -> Result<(), LayoutError> {
        self.engine.set_children(parent, children)?;
        for &child in children {
            link_parent(child, parent);
        }
        self.engine.mark_dirty(parent).ok();
        Ok(())
    }

    fn remove_node(&mut self, node: NodeId) {
        self.engine.remove(node);
        self.registry.remove(&node);
        // Every link naming `node` goes with it, not only the one it owned: taffy hands a freed index back out.
        with_parents(|p| {
            p.remove(&node);
            p.retain(|_, above| *above != node);
        });
        self.boundary_nodes.remove(&node);
        self.last_space.remove(&node);
        self.root_auto.remove(&node);
        self.abs_pos.remove(&node);
        self.abs_pos_signals.remove(&node);
    }
}

#[cfg(test)]
#[path = "context_test.rs"]
mod tests;

/// The CSS for what `node` was asked for, resolved against the direction in force.
///
/// For a backend whose output is a document rather than pixels: it is handed the box's intent, not the rect the intent produced, so the browser can lay the box out itself. `None` for a node this runtime does not own — a widget mid-teardown, or one built on another surface.
pub fn declared_css(node: NodeId) -> Option<layout_core::Css> {
    let direction = crate::direction::current_direction();
    with_runtime(|rt| {
        rt.engine
            .declared_style(node)
            .map(|style| style.to_css(direction))
    })
}
