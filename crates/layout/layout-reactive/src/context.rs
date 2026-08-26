use geometry_core::Rect;
use layout_core::{AvailableSpace, LayoutEngine, LayoutError, LayoutStyle, MeasureFn, NodeId};
use reactive_core::{RwSignal, batch, signal};
use rustc_hash::FxHashMap;

reactive_core::surface_local! {
    /// Which node each node hangs from — the tree's *shape*, kept in its own world rather than inside
    /// [`LAYOUT_RUNTIME`].
    ///
    /// It sits apart because of who has to read it and when: a measure closure runs *inside* the layout
    /// runtime's own borrow, and anything it reaches for that lives in that borrow is a re-entry. Text
    /// measured against a style its ancestors declared is exactly that reach, and the links it needs are
    /// structure, not layout state — nothing in a layout pass moves them.
    slot PARENTS: FxHashMap<NodeId, NodeId> = FxHashMap::default();
    access with_parents, with_parents_ref;
    context ParentsContext, ParentsGuard;
}

reactive_core::surface_local! {
    /// A per-surface layout tree: the taffy engine plus the node→rect-signal registry. The layout tree is
    /// a per-surface world so nodes can be created and laid out from anywhere — including reactive effects
    /// (reactive lists) that fire from an effect body. Under M3 several surfaces share one UI thread, so the
    /// runner activates each surface's [`LayoutContext`] around its build/event/frame; app code just calls
    /// the free functions, which operate on whichever surface is currently active.
    slot LAYOUT_RUNTIME: LayoutRuntime = LayoutRuntime::new();
    access with_runtime, with_runtime_ref;
    context LayoutContext, LayoutGuard;
}

/// Resets the active surface's layout runtime to a fresh, empty tree. The single-window app/preview harness
/// calls this at construction; a multi-surface runner instead gives each surface its own [`LayoutContext`].
pub fn reset_layout_runtime() {
    with_runtime(|rt| *rt = LayoutRuntime::new());
    with_parents(|p| p.clear());
}

pub fn new_leaf(style: LayoutStyle) -> Result<(NodeId, RwSignal<Rect>), LayoutError> {
    with_runtime(|rt| rt.new_leaf(style))
}

/// A leaf whose intrinsic size is computed by `measure` at layout time (e.g. text
/// whose height depends on how many lines it wraps into at the resolved width).
pub fn new_measured_leaf(
    style: LayoutStyle,
    measure: MeasureFn,
) -> Result<(NodeId, RwSignal<Rect>), LayoutError> {
    with_runtime(|rt| rt.new_measured_leaf(style, measure))
}

pub fn new_container(style: LayoutStyle, children: &[NodeId]) -> Result<NodeId, LayoutError> {
    with_runtime(|rt| rt.new_container(style, children))
}

/// Lays out `root` against the given space and reflects the result into each node's rect signal.
/// Collects the (signal, rect) updates while holding the runtime borrow, then applies them in a batch
/// *after* releasing it — a rect `.set()` can flush effects, and one of those may itself touch the
/// layout runtime (a reactive list), which would re-enter the borrow.
pub fn compute_layout(
    root: NodeId,
    width: AvailableSpace,
    height: AvailableSpace,
) -> Result<(), LayoutError> {
    // Reconciled here rather than in `set_direction` so the flip reaches every surface on the thread: the setter only knows about whichever one was active when it ran.
    let direction = crate::direction::current_direction();
    with_runtime(|rt| rt.engine.set_direction(direction));
    let updates = with_runtime(|rt| rt.compute_layout(root, width, height))?;
    batch(|| {
        for (sig, rect) in updates {
            if sig.peek() != rect {
                sig.set(rect);
            }
        }
    });
    Ok(())
}

/// Re-lays out every root that has been computed at least once, picking up any nodes a reactive change
/// dirtied since the last frame. Each `compute_layout` early-returns when its root is clean and the space
/// is unchanged, so this is cheap on a still frame. The runtime calls it once per redraw (after flushing
/// reactive effects, before rendering) so a data change deep in the tree — e.g. a reactive list adding an
/// item — is reflected in layout without the app shell knowing about it. Node dirtiness propagates up to
/// the root through taffy, so a dirtied list container makes its root recompute.
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

pub fn track_layout(node: NodeId) -> Option<RwSignal<Rect>> {
    with_runtime(|rt| rt.track_layout(node))
}

/// The node's WINDOW-absolute rect (top-left from the top-level walk, size from its layout), or `None` if it
/// has not been laid out under a window root yet. Unlike `track_layout`, this is correct even for a node in a
/// sub-root computed separately (whose rect signal is root-local) — use it to anchor a portaled overlay to a
/// trigger, since the portal hoists out of ancestor transforms and needs absolute coordinates.
///
/// This is the trigger's *laid-out* position. Scrolling moves content by a render transform, not by relaying
/// it out, so a node inside a scrolled viewport appears somewhere else on screen — `ui_core::visible_rect`
/// applies the offsets on top of this, which is what an anchored overlay wants.
pub fn absolute_rect(node: NodeId) -> Option<Rect> {
    with_runtime(|rt| {
        let &(x, y) = rt.abs_pos.get(&node)?;
        let size = rt.registry.get(&node).map(|s| s.peek()).unwrap_or_default();
        Some(Rect::new(x, y, size.width, size.height))
    })
}

/// The node `node` hangs from, or `None` at a root.
///
/// The **layout-tree** parent, which is not always the visual one: a portalled overlay is recorded against
/// the host it attached to, so anything walking up from inside one arrives where the markup put it rather
/// than where the compositor draws it. That is the link a cascade has to follow — CSS inherits through the
/// document, not through the stacking context — and it is the same one [`is_hidden`] already climbs.
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

/// Whether `node` is `ancestor` or sits anywhere beneath it. Follows the parent links the runtime records, so
/// it crosses into a separately-computed sub-root (a scroll's content) the way the layout tree does.
pub fn is_descendant_of(node: NodeId, ancestor: NodeId) -> bool {
    with_runtime(|rt| rt.is_in_subtree(node, ancestor))
}

/// Whether `node` is out of layout flow, by its own `display:none` or any ancestor's.
///
/// The climb is the point: taffy stops laying out under a hidden node, so a descendant keeps the size it had
/// when it was last shown and looks perfectly ordinary to anyone reading its rect. Only the chain says it is
/// gone. Follows the same parent links as [`is_descendant_of`], so it crosses into a portaled sub-root.
pub fn is_hidden(node: NodeId) -> bool {
    with_runtime(|rt| rt.is_hidden_by_display(node))
}

pub fn mark_dirty(node: NodeId) -> Result<(), LayoutError> {
    with_runtime(|rt| rt.engine.mark_dirty(node))
}

/// Replaces `node`'s layout style and dirties it, so the next pass lays it out again.
///
/// What a widget whose style is *derived* from reactive state calls when that state moves — a theme's metric
/// tokens, today. Unlike rebuilding the widget it keeps the node, its children, and everything they hold; the
/// engine re-resolves the new style against the current direction exactly as it would at construction.
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

/// Lays `node`'s children along the horizontal axis, after the node was built as a column. A reconciling
/// list boxed inside a `row` calls this: its own node exists before it is attached, so the direction it
/// should have cannot be known at construction.
pub fn set_container_row(node: NodeId) {
    with_runtime(|rt| rt.engine.make_flex_row(node));
    mark_dirty(node).ok();
}

/// Whether `node` is a flex row (main axis horizontal). A transparent `for … gap:N` fragment reads its host
/// container's axis to know which edge the per-item gap margin sits on.
pub fn container_is_row(node: NodeId) -> bool {
    with_runtime(|rt| rt.engine.is_row(node))
}

/// Sets `node`'s leading main-axis margin (`left` for a row host, `top` for a column) to `px` — the primitive
/// a transparent `for … gap:N` uses to space its items without a container of its own. Marks the node dirty.
pub fn set_leading_margin(node: NodeId, is_row: bool, px: f32) {
    with_runtime(|rt| rt.engine.set_leading_margin(node, is_row, px))
}

/// Sets `node`'s minimum height to `px` after the initial layout (dirtying it, which propagates up), so a
/// content-measured leaf grows to at least `px` even when its content is shorter. A scrolling editor uses it
/// to fill its viewport so a click anywhere in the empty area — not just over the text — lands on the leaf.
pub fn set_min_height(node: NodeId, px: f32) {
    with_runtime(|rt| rt.engine.set_min_height(node, Some(px)))
}

/// Replaces `parent`'s children with `children`, in order, marking `parent` dirty. Operates on the
/// thread-local runtime; `parent` must be a container already registered in the runtime.
pub fn set_children(parent: NodeId, children: &[NodeId]) -> Result<(), LayoutError> {
    with_runtime(|rt| rt.set_children(parent, children))
}

/// Detaches and frees `node` (a former list item) from the runtime: removes it from the layout tree and
/// drops its rect signal and bookkeeping. The caller must have removed it from its parent's child list
/// (via [`set_children`]) first.
pub fn remove_node(node: NodeId) {
    with_runtime(|rt| rt.remove_node(node))
}

/// Pins the overlay host to `node` — the app's window-spanning root — so overlays always fill the viewport
/// even when the app computes several independent layout roots (e.g. a shell with a separate sidebar root
/// computed after the main one, which the auto-detection would otherwise pick as the host). Call it each
/// relayout with the current main root (it survives hot-reload rebuilds, which mint a new root node). Once
/// pinned, auto-detection no longer overrides the host.
/// The area an overlay may occupy: the laid-out rect of the host its content is attached to, which is the
/// window (or the surface) it will be composed into.
///
/// What a panel needs to stay on screen. Without it an anchored bubble is placed from its trigger alone and
/// runs off whichever edge the trigger happens to be near — which is not a rare case but the common one, a
/// tooltip on the rightmost button of a toolbar.
pub fn overlay_viewport() -> Option<geometry_core::Rect> {
    with_runtime(|rt| {
        let host = rt.overlay_host?;
        rt.engine.layout(host).ok()
    })
}

pub fn set_overlay_host(node: NodeId) {
    with_runtime(|rt| {
        rt.overlay_host = Some(node);
        rt.host_pinned = true;
    });
}

/// Attaches `node` (an overlay's out-of-flow content) as an extra child of the current layout host — the
/// top-level root computed against the window — so it fills the viewport regardless of where the `overlay`
/// was declared in the tree. Returns `true` when attached; `false` when no host has been computed yet (the
/// caller then falls back to normal in-tree layout). The host is marked dirty so the next frame lays the
/// portal out.
pub fn attach_overlay(node: NodeId) -> bool {
    with_runtime(|rt| {
        let Some(host) = rt.overlay_host else {
            return false;
        };
        // A host sitting inside the content it would carry closes a parent cycle, and no root is reachable from under `node` afterwards. `overlay_host` is auto-detected and moves as sub-roots are laid out, so this needs nobody to ask for it; refuse, and the caller lays the overlay out in place.
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

/// Detaches an overlay's content from the layout host (inverse of [`attach_overlay`]); the caller frees it
/// afterwards with [`remove_node`]. A no-op if the host is gone.
pub fn detach_overlay(node: NodeId) {
    with_runtime(|rt| {
        // Remove from the host the overlay actually attached to (recorded in `parents` at attach), NOT the
        // current `overlay_host`: auto-detection may have moved the host to another root (e.g. a nested
        // scroll's content root) since attach, and taffy panics if `node` is not a child of the node removed.
        if let Some(host) = with_parents(|p| p.remove(&node)) {
            rt.engine.remove_child(host, node).ok();
            rt.engine.mark_dirty(host).ok();
        }
    });
}

struct LayoutRuntime {
    engine: LayoutEngine,
    registry: FxHashMap<NodeId, RwSignal<Rect>>,
    boundary_nodes: FxHashMap<NodeId, (f32, f32)>,
    // Available space each root was last computed against, so compute_layout can re-run when only the space changed (e.g. a window resize) even though the node itself is clean. Without this, resizing an independently-computed root is silently a no-op and its layout freezes at the first size.
    last_space: FxHashMap<NodeId, (AvailableSpace, AvailableSpace)>,
    // Nodes with a definite `max-width`, their original style, and the width pinned on the previous compute (`None` = unpinned). taffy sizes a max-width box's intrinsic height at its uncapped width, so a wrapping child reports a 1-line height and the box ends up too short. compute_layout pins each resolved width as a definite width and re-runs so heights are correct. The stored pin lets the undo pass stay idempotent: an unpinned box whose space did not change is left untouched.
    // Whether each compute-root's width/height were originally `auto`, captured the first time it is computed. An auto-sized root fills the definite space it is computed in, so a top-level page need not declare width:100% to avoid collapsing to its content width.
    root_auto: FxHashMap<NodeId, (bool, bool)>,
    // The parent-less (top-level) root last computed against the window — the layout host that `overlay`s
    // attach their out-of-flow content to, so a portal fills the viewport regardless of where it is declared.
    overlay_host: Option<NodeId>,
    // When set, `overlay_host` was pinned by the app via `set_overlay_host` and auto-detection (last
    // parent-less root wins) must NOT override it. An app with several independent roots (e.g. a shell with a
    // separate sidebar root computed after the main one) needs this: the window-spanning root is the host,
    // not whichever root happened to be computed last.
    host_pinned: bool,
    // Window-absolute top-left of each node, captured during the top-level (parent-less) root's walk (which
    // runs from the window origin, so its rects ARE window-absolute). Node rect SIGNALS stay root-local (a
    // sub-root computed separately, like the sandbox's scrolling `content`, leaves them content-local); this
    // map is the ONE place with window-absolute positions, so `absolute_rect` can anchor a portaled overlay
    // (which hoists out of ancestor transforms → needs absolute coords) to a trigger in a sub-root.
    abs_pos: FxHashMap<NodeId, (f32, f32)>,
    // Guards against recursive compute(): an effect that reads a layout signal and calls compute_layout() again creates a re-layout cycle caught immediately in debug builds.
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
            overlay_host: None,
            host_pinned: false,
            abs_pos: FxHashMap::default(),
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
        self.registry.insert(node, signal.clone());
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
        self.registry.insert(node, signal.clone());
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
    ) -> Result<Vec<(RwSignal<Rect>, Rect)>, LayoutError> {
        // A top-level root (no parent) computed against the window is the overlay host: overlays attach
        // their content here so a portal fills the viewport wherever it is declared. Refreshed each compute
        // so it stays current across a hot-reload rebuild (which mints a new root node). A definite height
        // marks the surface/window root; a detached sub-root laid out for its intrinsic height (a scroll's
        // content, computed with `MaxContent`) must NOT become the host, or a portal declared inside a scroll
        // would attach to that scroll and be torn down (and mis-detached) with it.
        if !self.host_pinned
            && with_parents_ref(|p| !p.contains_key(&root))
            && matches!(height, AvailableSpace::Definite(_))
        {
            self.overlay_host = Some(root);
        }
        // A changed available space (window resize) must re-run layout even when the node is clean: dirty the root so the cached size from the previous space is discarded. Skip only when both the node is clean and the space is unchanged.
        let is_space_changed = self.last_space.get(&root) != Some(&(width, height));
        if is_space_changed {
            self.engine.mark_dirty(root).ok();
            self.last_space.insert(root, (width, height));
        } else if !self.engine.is_dirty(root) {
            return Ok(Vec::new());
        }
        // A layout root fills the definite space it is computed in: an auto width or height becomes the available size, so a top-level page need not declare width:100% to avoid collapsing to its content. Only this root is affected.
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
        // Collect the changed rects while holding the runtime borrow, but apply them (`sig.set`) only
        // after the caller releases it: a set flushes effects, one of which may re-enter the runtime.
        let mut updates: Vec<(RwSignal<Rect>, Rect)> = Vec::new();
        // Only a full walk of a parent-less root runs from the window origin, so only then are the walked
        // rects window-absolute. A sub-boundary or sub-root walk is root-local — don't capture those.
        let is_window_walk = layout_root == root && with_parents_ref(|p| !p.contains_key(&root));
        let mut abs_updates: Vec<(NodeId, f32, f32)> = Vec::new();
        let registry = &self.registry;
        let walk_result = self.engine.walk(layout_root, &mut |node_id, rect| {
            if let Some(sig) = registry.get(&node_id) {
                if sig.peek() != rect {
                    updates.push((sig.clone(), rect));
                }
            }
            if is_window_walk {
                abs_updates.push((node_id, rect.x, rect.y));
            }
            true
        });
        for (n, x, y) in abs_updates {
            self.abs_pos.insert(n, (x, y));
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
        // Every link *naming* `node` goes with it, not only the one it owned. Taffy hands a freed index back out under a new generation, so a link left pointing here does not dangle harmlessly — it comes to name whichever node is minted next, and a climb that should have reached a root finds a cycle instead. `retain` drops only the links that still name it, so a child re-parented since keeps where it moved.
        with_parents(|p| {
            p.remove(&node);
            p.retain(|_, above| *above != node);
        });
        self.boundary_nodes.remove(&node);
        self.last_space.remove(&node);
        self.root_auto.remove(&node);
        self.abs_pos.remove(&node);
    }
}

#[cfg(test)]
mod tests {
    use geometry_core::Rect;
    use layout_core::{LayoutStyle, SizeDimension};

    use super::*;

    // These hang rather than fail when the guard is gone, which is how the bug reached a desktop: the shell burned a core in `inherit::context` from its first frame and never drew.
    #[test]
    fn an_overlay_is_refused_a_host_that_sits_inside_it() {
        reset_layout_runtime();
        let inner = new_container(LayoutStyle::new(), &[]).unwrap();
        let content = new_container(LayoutStyle::new(), &[inner]).unwrap();
        // What auto-detection can leave behind once a sub-root of the overlay has been laid out on its own.
        set_overlay_host(inner);

        assert!(
            !attach_overlay(content),
            "a host beneath the content it would carry closes a parent cycle"
        );
        assert_eq!(
            parent(content),
            None,
            "a refused attach leaves the content parentless rather than half-linked"
        );
    }

    #[test]
    fn removing_a_node_leaves_no_link_pointing_at_it() {
        reset_layout_runtime();
        let child = new_container(LayoutStyle::new(), &[]).unwrap();
        let host = new_container(LayoutStyle::new(), &[child]).unwrap();
        assert_eq!(parent(child), Some(host));

        remove_node(host);

        assert_eq!(
            parent(child),
            None,
            "a link to a freed node outlives it and then names whatever reuses the id"
        );
    }

    #[test]
    fn a_cycle_in_the_parent_links_ends_the_climb_instead_of_spinning() {
        reset_layout_runtime();
        let a = new_container(LayoutStyle::new(), &[]).unwrap();
        let b = new_container(LayoutStyle::new(), &[]).unwrap();
        let unrelated = new_container(LayoutStyle::new(), &[]).unwrap();
        with_parents(|p| {
            p.insert(a, b);
            p.insert(b, a);
        });

        let climbed: Vec<_> = ancestors(a).collect();
        assert!(
            climbed.iter().all(|node| *node == a || *node == b),
            "the climb stays inside the cycle it found: {climbed:?}"
        );
        assert!(
            !is_descendant_of(a, unrelated),
            "a question asked from inside a cycle answers instead of hanging"
        );
    }

    // A child whose HEIGHT depends on the width it is given, which is the case a `max-width` box used to get
    // wrong: taffy 0.11 measured it at its uncapped one-line intrinsic width, so `layout-reactive` pinned
    // every capped box to its resolved width and laid out a second time. That pass is gone — the behaviour
    // went with taffy 0.13, and `telar/tests/max_width_wrapping.rs` is the same case through the real shaper.
    #[test]
    fn a_maxwidth_box_measures_its_content_at_the_capped_width() {
        reset_layout_runtime();
        const TOTAL: f32 = 1200.0;
        const LINE: f32 = 20.0;
        // Wrapping text: at whatever width it is offered, it needs `TOTAL / width` lines of `LINE` height.
        let measure: layout_core::MeasureFn = Box::new(|available: f32| {
            let width = if available > 0.0 { available } else { TOTAL };
            (width, (TOTAL / width).ceil() * LINE)
        });
        let (text, text_rect) = new_measured_leaf(LayoutStyle::new(), measure).unwrap();
        let box_node =
            new_container(LayoutStyle::new().flex_column().max_width(400.0), &[text]).unwrap();
        let root = new_container(LayoutStyle::new().flex_column(), &[box_node]).unwrap();
        let box_rect = track_layout(box_node).unwrap();

        compute_layout(
            root,
            AvailableSpace::Definite(1000.0),
            AvailableSpace::Definite(1000.0),
        )
        .unwrap();

        assert_eq!(text_rect.get().width, 400.0);
        assert_eq!(
            box_rect.get().height,
            3.0 * LINE,
            "the box has to reserve the height its content takes at the width it was capped to"
        );
    }

    // A flex-wrap row nested in a max-width box (the full-bleed-band + centered- content pattern) must reserve height for the lines it actually wraps into, even though taffy would otherwise size the box at its uncapped 1-line width.
    #[test]
    fn maxwidth_box_reserves_height_for_wrapped_content() {
        reset_layout_runtime();
        let mut items = Vec::new();
        for _ in 0..4 {
            let (n, _) = new_leaf(
                LayoutStyle::new()
                    .width(200.0)
                    .height(100.0)
                    .min_width(200.0)
                    .flex_grow(1.0),
            )
            .unwrap();
            items.push(n);
        }
        let row =
            new_container(LayoutStyle::new().flex_row().flex_wrap().gap(24.0), &items).unwrap();
        // Capped to 500 → 2 items per row → the 4 items wrap onto 2 lines.
        let boxed = new_container(
            LayoutStyle::new()
                .flex_column()
                .width(SizeDimension::Percent(1.0))
                .max_width(500.0),
            &[row],
        )
        .unwrap();
        let page = new_container(
            LayoutStyle::new()
                .flex_column()
                .width(SizeDimension::Percent(1.0)),
            &[boxed],
        )
        .unwrap();
        compute_layout(
            page,
            AvailableSpace::Definite(900.0),
            AvailableSpace::MaxContent,
        )
        .unwrap();
        let box_rect = track_layout(boxed).unwrap().get();
        let row_rect = track_layout(row).unwrap().get();
        assert!(
            (box_rect.width - 500.0).abs() < 1.0,
            "box not capped: {box_rect:?}"
        );
        assert!(
            row_rect.height >= 200.0,
            "row did not wrap to 2 lines: {row_rect:?}"
        );
        assert!(
            box_rect.height >= row_rect.height - 0.5,
            "box too short for wrapped content: box={box_rect:?} row={row_rect:?}"
        );
    }

    // A wrapping flex row must reserve height for ALL its lines so a following sibling sits below it instead
    // of overlapping. Reproduces the "next section positions as if the wrapped card didn't exist" report.
    #[test]
    fn wrapped_flex_row_reserves_height_for_all_lines() {
        reset_layout_runtime();
        let mut cards = Vec::new();
        for _ in 0..4 {
            let (n, _) = new_leaf(
                LayoutStyle::new()
                    .min_width(260.0)
                    .height(100.0)
                    .flex_grow(1.0),
            )
            .unwrap();
            cards.push(n);
        }
        let row =
            new_container(LayoutStyle::new().flex_row().flex_wrap().gap(24.0), &cards).unwrap();
        let (marker, _) = new_leaf(LayoutStyle::new().height(50.0)).unwrap();
        let col =
            new_container(LayoutStyle::new().flex_column().gap(20.0), &[row, marker]).unwrap();
        // 900px wide → 3 cards on line 1, the 4th wraps to line 2.
        compute_layout(
            col,
            AvailableSpace::Definite(900.0),
            AvailableSpace::MaxContent,
        )
        .unwrap();
        let row_rect = track_layout(row).unwrap().get();
        let marker_rect = track_layout(marker).unwrap().get();
        assert!(
            row_rect.height >= 220.0,
            "wrapped row height {} should cover 2 lines (~224)",
            row_rect.height
        );
        assert!(
            marker_rect.y >= row_rect.y + row_rect.height - 0.5,
            "marker overlaps wrapped row: row.y={} row.h={} marker.y={}",
            row_rect.y,
            row_rect.height,
            marker_rect.y
        );
    }

    // Same as above but the cards are content-sized containers (a column whose height comes from its
    // children) with grow:1 — the real feature-card shape.
    #[test]
    fn wrapped_content_sized_cards_reserve_height() {
        reset_layout_runtime();
        let mut cards = Vec::new();
        for _ in 0..4 {
            let (inner, _) = new_leaf(LayoutStyle::new().height(100.0)).unwrap();
            let card = new_container(
                LayoutStyle::new()
                    .flex_column()
                    .min_width(260.0)
                    .flex_grow(1.0),
                &[inner],
            )
            .unwrap();
            cards.push(card);
        }
        let row =
            new_container(LayoutStyle::new().flex_row().flex_wrap().gap(24.0), &cards).unwrap();
        let (marker, _) = new_leaf(LayoutStyle::new().height(50.0)).unwrap();
        let col =
            new_container(LayoutStyle::new().flex_column().gap(20.0), &[row, marker]).unwrap();
        compute_layout(
            col,
            AvailableSpace::Definite(900.0),
            AvailableSpace::MaxContent,
        )
        .unwrap();
        let row_rect = track_layout(row).unwrap().get();
        let marker_rect = track_layout(marker).unwrap().get();
        assert!(
            row_rect.height >= 220.0,
            "wrapped content-sized row height {} should cover 2 lines (~224)",
            row_rect.height
        );
        assert!(
            marker_rect.y >= row_rect.y + row_rect.height - 0.5,
            "marker overlaps row: row.y={} row.h={} marker.y={}",
            row_rect.y,
            row_rect.height,
            marker_rect.y
        );
    }

    // Re-running compute_layout against the SAME available space (root re-dirtied by an unrelated change) must leave the max-width box where it was: a layout that drifts between two identical inputs is one nobody can reason about.
    #[test]
    fn maxwidth_box_stable_across_recompute() {
        reset_layout_runtime();
        let mut items = Vec::new();
        for _ in 0..4 {
            let (n, _) = new_leaf(
                LayoutStyle::new()
                    .width(200.0)
                    .height(100.0)
                    .min_width(200.0)
                    .flex_grow(1.0),
            )
            .unwrap();
            items.push(n);
        }
        let row =
            new_container(LayoutStyle::new().flex_row().flex_wrap().gap(24.0), &items).unwrap();
        let boxed = new_container(
            LayoutStyle::new()
                .flex_column()
                .width(SizeDimension::Percent(1.0))
                .max_width(500.0),
            &[row],
        )
        .unwrap();
        let page = new_container(
            LayoutStyle::new()
                .flex_column()
                .width(SizeDimension::Percent(1.0)),
            &[boxed],
        )
        .unwrap();

        let space = (AvailableSpace::Definite(900.0), AvailableSpace::MaxContent);
        compute_layout(page, space.0, space.1).unwrap();
        let first = track_layout(boxed).unwrap().get();

        // Re-dirty the root and recompute at the SAME space: exercises the idempotent undo on an already-pinned box.
        mark_dirty(page).unwrap();
        compute_layout(page, space.0, space.1).unwrap();
        let second = track_layout(boxed).unwrap().get();

        assert!(
            (second.width - 500.0).abs() < 1.0,
            "box not capped on recompute: {second:?}"
        );
        assert!(
            (first.width - second.width).abs() < 0.5 && (first.height - second.height).abs() < 0.5,
            "box layout drifted across recompute: first={first:?} second={second:?}"
        );
    }

    // A max-width box hidden out of band stays hidden across a resize. The pin pass used to restore each capped node's whole construction-time style, taking an out-of-band `set_display(false)` with it.
    #[test]
    fn hidden_maxwidth_box_stays_hidden_after_a_resize() {
        reset_layout_runtime();
        let mut items = Vec::new();
        for _ in 0..4 {
            let (n, _) = new_leaf(
                LayoutStyle::new()
                    .width(200.0)
                    .height(100.0)
                    .min_width(200.0)
                    .flex_grow(1.0),
            )
            .unwrap();
            items.push(n);
        }
        let row =
            new_container(LayoutStyle::new().flex_row().flex_wrap().gap(24.0), &items).unwrap();
        let boxed = new_container(
            LayoutStyle::new()
                .flex_column()
                .width(SizeDimension::Percent(1.0))
                .max_width(500.0),
            &[row],
        )
        .unwrap();
        let page = new_container(
            LayoutStyle::new()
                .flex_column()
                .width(SizeDimension::Percent(1.0)),
            &[boxed],
        )
        .unwrap();

        compute_layout(
            page,
            AvailableSpace::Definite(900.0),
            AvailableSpace::MaxContent,
        )
        .unwrap();

        set_display(boxed, false);
        mark_dirty(page).unwrap();
        assert!(is_hidden(boxed), "hidden right after set_display");

        // The undo pass lifts boxed's previous width pin here (available space changed); must not also un-hide it.
        compute_layout(
            page,
            AvailableSpace::Definite(700.0),
            AvailableSpace::MaxContent,
        )
        .unwrap();
        assert!(
            is_hidden(boxed),
            "resize must not revert the out-of-band hide"
        );
    }

    // An auto-sized layout root fills the definite space it is computed in, so a page need not declare width:100% to avoid collapsing to its content width.
    #[test]
    fn auto_root_fills_definite_width() {
        reset_layout_runtime();
        let (child, _) = new_leaf(LayoutStyle::new().height(40.0)).unwrap();
        // A column with auto width whose child is content-sized would otherwise shrink to the child; the root-fill rule stretches it to the given width.
        let page = new_container(LayoutStyle::new().flex_column(), &[child]).unwrap();
        compute_layout(
            page,
            AvailableSpace::Definite(1000.0),
            AvailableSpace::MaxContent,
        )
        .unwrap();
        let w = track_layout(page).unwrap().get().width;
        assert!(
            (w - 1000.0).abs() < 1.0,
            "auto root did not fill width: {w}"
        );
    }

    // Repro: an auto-width root that ALSO carries max_width must still fill the definite space (capped by max_width), not collapse to its content width.
    #[test]
    fn hidden_child_collapses_to_zero_rect() {
        // A section toggled to `display:none` must collapse to a zero rect so its view draws nothing and
        // does not overlap the visible section (the tab-switch mechanism in the sandbox relies on this).
        reset_layout_runtime();
        let (a, _) = new_leaf(LayoutStyle::new().width(50.0).height(30.0)).unwrap();
        let (b, b_rect) = new_leaf(LayoutStyle::new().width(50.0).height(30.0)).unwrap();
        let root = new_container(LayoutStyle::new().flex_column(), &[a, b]).unwrap();
        compute_layout(
            root,
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(200.0),
        )
        .unwrap();
        assert!(b_rect.get().height > 0.0, "b should start visible");

        set_display(b, false);
        mark_dirty(root).unwrap();
        compute_layout(
            root,
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(200.0),
        )
        .unwrap();
        let r = b_rect.get();
        assert_eq!(
            (r.width, r.height),
            (0.0, 0.0),
            "hidden child not collapsed: {r:?}"
        );
    }

    // Hiding a section must collapse its whole subtree, not just the section node: taffy leaves stale
    // layouts on descendants of a `display:none` node, so without zeroing them a Canvas (which paints at
    // fixed coordinates) in a hidden section would still draw over the visible one.
    #[test]
    fn hidden_subtree_collapses_descendants() {
        reset_layout_runtime();
        let (grandchild, gc_rect) = new_leaf(LayoutStyle::new().width(40.0).height(20.0)).unwrap();
        let section = new_container(LayoutStyle::new().flex_column(), &[grandchild]).unwrap();
        let root = new_container(LayoutStyle::new().flex_column(), &[section]).unwrap();
        compute_layout(
            root,
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(200.0),
        )
        .unwrap();
        assert!(gc_rect.get().width > 0.0, "grandchild should start visible");

        set_display(section, false);
        mark_dirty(root).unwrap();
        compute_layout(
            root,
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(200.0),
        )
        .unwrap();
        let r = gc_rect.get();
        assert_eq!(
            (r.width, r.height),
            (0.0, 0.0),
            "descendant of hidden section not collapsed: {r:?}"
        );
    }

    #[test]
    fn auto_root_with_max_width_fills_capped() {
        reset_layout_runtime();
        let (child, _) = new_leaf(LayoutStyle::new().height(40.0)).unwrap();
        let page =
            new_container(LayoutStyle::new().flex_column().max_width(600.0), &[child]).unwrap();
        // Wider than the cap: should fill up to max_width (600), not shrink to content (0).
        compute_layout(
            page,
            AvailableSpace::Definite(1000.0),
            AvailableSpace::MaxContent,
        )
        .unwrap();
        let w = track_layout(page).unwrap().get().width;
        assert!((w - 600.0).abs() < 1.0, "capped fill failed: {w}");
        // Narrower than the cap: should fill the available width (400).
        compute_layout(
            page,
            AvailableSpace::Definite(400.0),
            AvailableSpace::MaxContent,
        )
        .unwrap();
        let w = track_layout(page).unwrap().get().width;
        assert!((w - 400.0).abs() < 1.0, "sub-cap fill failed: {w}");
    }

    // The landing/sandbox shell pattern: an auto-width outer that fills and centers a capped inner column.
    #[test]
    fn centered_capped_column_tracks_width() {
        reset_layout_runtime();
        let (child, _) = new_leaf(LayoutStyle::new().height(40.0)).unwrap();
        let inner = new_container(
            LayoutStyle::new()
                .flex_column()
                .width(SizeDimension::Percent(1.0))
                .max_width(960.0),
            &[child],
        )
        .unwrap();
        let outer = new_container(
            LayoutStyle::new()
                .flex_column()
                .align_items(layout_core::AlignItems::CENTER),
            &[inner],
        )
        .unwrap();
        let inner_rect = track_layout(inner).unwrap();
        let outer_rect = track_layout(outer).unwrap();
        // Wide window: outer fills 1400, inner caps at 960 and is centered ((1400-960)/2 = 220).
        compute_layout(
            outer,
            AvailableSpace::Definite(1400.0),
            AvailableSpace::MaxContent,
        )
        .unwrap();
        assert!(
            (outer_rect.get().width - 1400.0).abs() < 1.0,
            "outer fill: {}",
            outer_rect.get().width
        );
        assert!(
            (inner_rect.get().width - 960.0).abs() < 1.0,
            "inner cap: {}",
            inner_rect.get().width
        );
        assert!(
            (inner_rect.get().x - 220.0).abs() < 1.0,
            "inner centered: {}",
            inner_rect.get().x
        );
        // Narrow window: inner fills the full width and centering adds no margin.
        compute_layout(
            outer,
            AvailableSpace::Definite(700.0),
            AvailableSpace::MaxContent,
        )
        .unwrap();
        assert!(
            (inner_rect.get().width - 700.0).abs() < 1.0,
            "inner tracks narrow: {}",
            inner_rect.get().width
        );
        assert!(
            inner_rect.get().x.abs() < 1.0,
            "no margin when full: {}",
            inner_rect.get().x
        );
    }

    // set_min_height grows a content-measured leaf to fill a viewport it would otherwise underflow (the
    // notebook editor's fill-the-viewport trick); a leaf whose content already exceeds the floor is untouched.
    #[test]
    fn set_min_height_grows_short_measured_leaf() {
        reset_layout_runtime();
        // A measured leaf reporting a fixed 20px content height, like a one-line text area.
        let (leaf, rect) = new_measured_leaf(
            LayoutStyle::new().width(SizeDimension::Percent(1.0)),
            Box::new(|_w| (0.0, 20.0)),
        )
        .unwrap();
        let root = new_container(
            LayoutStyle::new()
                .flex_column()
                .width(SizeDimension::Percent(1.0)),
            &[leaf],
        )
        .unwrap();
        let space = (AvailableSpace::Definite(300.0), AvailableSpace::MaxContent);
        compute_layout(root, space.0, space.1).unwrap();
        assert!(
            (rect.get().height - 20.0).abs() < 0.5,
            "starts at its content height: {:?}",
            rect.get()
        );

        // Grown to 200: the leaf now fills that height even though its content is only 20px tall.
        set_min_height(leaf, 200.0);
        compute_layout(root, space.0, space.1).unwrap();
        assert!(
            (rect.get().height - 200.0).abs() < 0.5,
            "min_height fills the short leaf: {:?}",
            rect.get()
        );

        // Cleared back to auto: the leaf collapses to its content height again.
        set_min_height(leaf, 0.0);
        compute_layout(root, space.0, space.1).unwrap();
        assert!(
            (rect.get().height - 20.0).abs() < 0.5,
            "a zero floor restores the content height: {:?}",
            rect.get()
        );
    }

    // Regression: set_layout_style (what a styled_by closure calls on every reactive re-run) used to overwrite the node's whole style, discarding an unrelated out-of-band set_min_height. No max_width involved.
    #[test]
    fn min_height_survives_a_later_set_layout_style() {
        reset_layout_runtime();
        let (leaf, rect) = new_measured_leaf(
            LayoutStyle::new().width(SizeDimension::Percent(1.0)),
            Box::new(|_w| (0.0, 20.0)),
        )
        .unwrap();
        let root = new_container(
            LayoutStyle::new()
                .flex_column()
                .width(SizeDimension::Percent(1.0)),
            &[leaf],
        )
        .unwrap();
        let space = (AvailableSpace::Definite(300.0), AvailableSpace::MaxContent);
        compute_layout(root, space.0, space.1).unwrap();

        set_min_height(leaf, 200.0);
        compute_layout(root, space.0, space.1).unwrap();
        assert!(
            (rect.get().height - 200.0).abs() < 0.5,
            "min_height applied: {:?}",
            rect.get()
        );

        // A wholesale restyle unrelated to min_height must not discard the floor.
        set_layout_style(leaf, LayoutStyle::new().width(SizeDimension::Percent(1.0))).unwrap();
        compute_layout(root, space.0, space.1).unwrap();
        assert!(
            (rect.get().height - 200.0).abs() < 0.5,
            "min_height survives a later set_layout_style: {:?}",
            rect.get()
        );
    }

    #[test]
    fn ctx_register_leaf_returns_ok() {
        reset_layout_runtime();
        let result = new_leaf(LayoutStyle::new());
        assert!(result.is_ok());
    }

    #[test]
    fn ctx_new_container_returns_ok() {
        reset_layout_runtime();
        let leaf_result = new_leaf(LayoutStyle::new());
        assert!(leaf_result.is_ok());
        let (leaf, _) = leaf_result.unwrap();
        let container_result = new_container(LayoutStyle::new(), &[leaf]);
        assert!(container_result.is_ok());
    }

    #[test]
    fn ctx_register_leaf_returns_zero_rect() {
        reset_layout_runtime();
        let (_node, rect) = new_leaf(LayoutStyle::new()).unwrap();
        assert_eq!(rect.get(), Rect::default());
    }

    #[test]
    fn ctx_compute_updates_rect() {
        reset_layout_runtime();
        let (leaf, rect) = new_leaf(LayoutStyle::new().width(100.0).height(50.0)).unwrap();
        let root = new_container(
            LayoutStyle::new().flex_row().width(200.0).height(100.0),
            &[leaf],
        )
        .unwrap();
        compute_layout(
            root,
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();
        assert_eq!(rect.get().width, 100.0);
        assert_eq!(rect.get().height, 50.0);
    }

    #[test]
    fn setting_the_direction_signal_reaches_the_engine_on_the_next_layout_pass() {
        // Nothing rebuilds, so the rect signals asserted here are the same ones the widgets already hold.
        reset_layout_runtime();
        crate::set_direction(layout_core::Direction::Ltr);
        let (first, first_rect) = new_leaf(LayoutStyle::new().width(40.0).height(10.0)).unwrap();
        let (second, second_rect) = new_leaf(LayoutStyle::new().width(40.0).height(10.0)).unwrap();
        let root = new_container(
            LayoutStyle::new().flex_row().width(200.0).height(100.0),
            &[first, second],
        )
        .unwrap();
        let space = || {
            (
                AvailableSpace::Definite(200.0),
                AvailableSpace::Definite(100.0),
            )
        };
        let (w, h) = space();
        compute_layout(root, w, h).unwrap();
        assert_eq!(first_rect.get().x, 0.0);
        assert_eq!(second_rect.get().x, 40.0);

        crate::set_direction(layout_core::Direction::Rtl);
        mark_dirty(root).unwrap();
        let (w, h) = space();
        compute_layout(root, w, h).unwrap();
        assert_eq!(first_rect.get().x, 160.0, "the row now starts at the right");
        assert_eq!(second_rect.get().x, 120.0);
        crate::set_direction(layout_core::Direction::Ltr);
    }

    // An overlay's content, attached to the host, fills the viewport — not the small box it was declared in.
    #[test]
    fn attached_overlay_fills_host_viewport_not_its_small_parent() {
        reset_layout_runtime();
        // Computing a parent-less root registers it as the overlay host (an 800×600 viewport).
        let (small, _) = new_leaf(LayoutStyle::new().width(50.0).height(50.0)).unwrap();
        let root = new_container(LayoutStyle::new().flex_column(), &[small]).unwrap();
        compute_layout(
            root,
            AvailableSpace::Definite(800.0),
            AvailableSpace::Definite(600.0),
        )
        .unwrap();

        // Overlay content: an absolute-fill container with a 100%×100% inner leaf we can measure.
        let (inner, inner_rect) = new_leaf(
            LayoutStyle::new()
                .width(SizeDimension::Percent(1.0))
                .height(SizeDimension::Percent(1.0)),
        )
        .unwrap();
        let content = new_container(LayoutStyle::new().absolute_fill(), &[inner]).unwrap();
        assert!(
            attach_overlay(content),
            "the host must be set after the first compute"
        );
        relayout_if_dirty();

        let r = inner_rect.get();
        assert!(
            (r.width - 800.0).abs() < 0.5 && (r.height - 600.0).abs() < 0.5,
            "portal fills the viewport, not its 50px parent: {r:?}"
        );

        // Detaching and freeing the content must leave the host laying out cleanly (no panic, still valid).
        detach_overlay(content);
        remove_node(content);
        relayout_if_dirty();
    }

    // Reproduces the sandbox shell's coordinate trap: a `[sidebar | content]` window root, then the `content`
    // computed AGAIN as its own root (for scroll-height measurement) — which rewrites the content subtree's
    // rect signals to content-local coords. `absolute_rect` must still report a trigger's WINDOW-absolute
    // position (past the sidebar), so a portaled dropdown anchors correctly instead of landing over the sidebar.
    #[test]
    fn absolute_rect_stays_window_absolute_across_a_separate_content_root() {
        reset_layout_runtime();
        let (sidebar, _) = new_leaf(LayoutStyle::new().width(248.0).height(600.0)).unwrap();
        let (trigger, trigger_sig) =
            new_leaf(LayoutStyle::new().width(120.0).height(30.0)).unwrap();
        let content =
            new_container(LayoutStyle::new().flex_column().flex_grow(1.0), &[trigger]).unwrap();
        let root = new_container(LayoutStyle::new().flex_row(), &[sidebar, content]).unwrap();
        compute_layout(
            root,
            AvailableSpace::Definite(1000.0),
            AvailableSpace::Definite(600.0),
        )
        .unwrap();
        set_overlay_host(root);
        // The trigger is at window x ≈ 248 (immediately right of the sidebar).
        assert!(
            (absolute_rect(trigger).unwrap().x - 248.0).abs() < 1.0,
            "abs x should be past the 248px sidebar: {:?}",
            absolute_rect(trigger)
        );

        // Compute `content` as its own root (the sandbox does this for scroll height): the SIGNAL goes local.
        mark_dirty(content).unwrap();
        compute_layout(
            content,
            AvailableSpace::Definite(752.0),
            AvailableSpace::MaxContent,
        )
        .unwrap();
        assert!(
            trigger_sig.get().x < 1.0,
            "the rect signal is now content-local (~0): {:?}",
            trigger_sig.get()
        );
        // But absolute_rect still reports window-absolute (past the sidebar) — this is the fix.
        assert!(
            (absolute_rect(trigger).unwrap().x - 248.0).abs() < 1.0,
            "absolute_rect must stay window-absolute across the sub-root compute: {:?}",
            absolute_rect(trigger)
        );
    }
}
