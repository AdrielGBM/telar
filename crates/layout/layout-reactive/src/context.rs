//! The layout runtime the widget tree drives: node rects as signals, dirty roots, and the overlay host.

use std::cell::RefCell;
use std::mem::ManuallyDrop;

use geometry_core::Rect;
use layout_core::{AvailableSpace, LayoutEngine, LayoutError, LayoutStyle, MeasureFn, NodeId};
use reactive_core::{OwnerId, RwSignal, SurfaceHandle, batch, signal};
use rustc_hash::{FxHashMap, FxHashSet};

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
    let created = with_runtime(|rt| rt.new_leaf(style))?;
    note_created(created.0);
    Ok(created)
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
    let created = with_runtime(|rt| rt.new_measured_leaf(style, measure))?;
    note_created(created.0);
    Ok(created)
}

/// Creates a container node holding `children`.
pub fn new_container(style: LayoutStyle, children: &[NodeId]) -> Result<NodeId, LayoutError> {
    let node = with_runtime(|rt| rt.new_container(style, children))?;
    note_created(node);
    Ok(node)
}

// `ManuallyDrop` for the reason every TLS slot here carries it: a destructor registered from the app dylib makes `dlclose` unsafe.
thread_local! {
    /// Every open recording, innermost last.
    static RECORDINGS: ManuallyDrop<RefCell<Vec<Recording>>> =
        const { ManuallyDrop::new(RefCell::new(Vec::new())) };
}

/// The surface is part of a recording's claim because node ids are per-surface: a write during a build can flush an effect on another surface, and the ids that effect mints name nodes in a tree this recording cannot free.
struct Recording {
    root: Option<OwnerId>,
    surface: SurfaceHandle,
    nodes: Vec<(NodeId, Option<OwnerId>)>,
}

impl Recording {
    /// Only a node built under the recording's owner is its own: a write during a build can flush effects elsewhere in the tree, and what they create is not the build's to free.
    fn claims(&self, surface: SurfaceHandle, owner: Option<OwnerId>) -> bool {
        if surface != self.surface {
            return false;
        }
        match (self.root, owner) {
            (None, _) => true,
            (Some(root), Some(owner)) => reactive_core::owner_within(owner, root),
            (Some(_), None) => false,
        }
    }
}

fn note_created(node: NodeId) {
    let owner = reactive_core::current_owner();
    let surface = reactive_core::current_surface();
    RECORDINGS.with(|stack| {
        if let Some(innermost) = stack.borrow_mut().last_mut()
            && innermost.claims(surface, owner)
        {
            innermost.nodes.push((node, owner));
        }
    });
}

/// Starts recording the nodes created under the current owner from here on, so a build that panics or returns an error partway can free the ones nothing had attached yet — a disposed subtree only frees what hangs from its root.
pub fn record_nodes() -> NodeRecording {
    let depth = RECORDINGS.with(|stack| {
        let mut stack = stack.borrow_mut();
        stack.push(Recording {
            root: reactive_core::current_owner(),
            surface: reactive_core::current_surface(),
            nodes: Vec::new(),
        });
        stack.len() - 1
    });
    NodeRecording { depth, kept: false }
}

/// An open recording from [`record_nodes`]. Dropped without [`keep`](Self::keep), including by an unwind, it frees every node recorded since it opened, with whatever hangs from them.
#[must_use = "dropping the recording frees what it recorded"]
pub struct NodeRecording {
    depth: usize,
    kept: bool,
}

impl NodeRecording {
    /// Leaves the recorded nodes alive. The recording this one sits inside takes over the ones it would have recorded itself — built on its surface, under its owner — and no others.
    pub fn keep(mut self) {
        self.kept = true;
    }

    fn close(&self) -> Option<Recording> {
        RECORDINGS.with(|stack| {
            let mut stack = stack.borrow_mut();
            if stack.len() <= self.depth {
                return None;
            }
            stack.drain(self.depth..).next()
        })
    }
}

impl Drop for NodeRecording {
    fn drop(&mut self) {
        let Some(recording) = self.close() else {
            return;
        };
        if self.kept {
            RECORDINGS.with(|stack| {
                if let Some(enclosing) = stack.borrow_mut().last_mut() {
                    let handed: Vec<_> = recording
                        .nodes
                        .into_iter()
                        .filter(|&(_, owner)| enclosing.claims(recording.surface, owner))
                        .collect();
                    enclosing.nodes.extend(handed);
                }
            });
            return;
        }
        if recording.nodes.is_empty() {
            return;
        }
        let _entered = recording.surface.enter();
        // A surface torn down while this was open took its tree with it, and its ids now name nodes in whichever tree is active.
        if reactive_core::current_surface() != recording.surface {
            return;
        }
        for (node, _) in recording.nodes {
            remove_node(node);
        }
    }
}

/// Lays out `root` against the given space and reflects the result into the signals that watch it. Collects the updates while holding the runtime borrow, then applies them in a batch *after* releasing it — a `.set()` can flush effects, and one of those may itself touch the layout runtime (a reactive list), which would re-enter the borrow.
pub fn compute_layout(
    root: NodeId,
    width: AvailableSpace,
    height: AvailableSpace,
) -> Result<(), LayoutError> {
    // Reconciled here rather than in `set_direction`, so the flip reaches every surface on the thread; the size is the active surface's own, reconciled on the same pass so a resize and the layout answering it agree.
    let direction = crate::direction::current_direction();
    let surface = crate::surface_size::surface_size();
    with_runtime(|rt| {
        rt.engine.set_direction(direction);
        rt.engine.set_surface_size(surface);
    });
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

/// How many layout passes the active surface has run: computes that found something dirty and laid it out, not the calls that found everything clean.
pub fn layout_passes() -> u64 {
    with_runtime_ref(|rt| rt.passes)
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
            .or_insert_with(|| reactive_core::in_surface_world(|| reactive_core::signal(position)));
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

/// Whether `node` is `ancestor` or sits anywhere beneath it.
///
/// Follows the parent links the runtime records, which is exactly as far as the layout tree reaches: a portaled overlay is linked to the host it attached to, so the climb crosses into one — but a scroll's content is a root of its own with no link to its viewport at all, so nothing inside one is a descendant of the scroll area that shows it. What bridges that gap is the input registry, which records its own link from the content to the viewport and climbs that instead.
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

/// Frees `node` and every node beneath it, with their rect signals and bookkeeping. The caller must have removed it from its parent's child list (via [`set_children`]) first; a node already freed is skipped.
pub fn remove_node(node: NodeId) {
    with_runtime(|rt| rt.remove_node(node))
}

/// How many layout nodes the active surface's runtime holds. What a test watches to see a disposal free every node it should.
pub fn live_node_count() -> usize {
    with_runtime_ref(|rt| rt.engine.node_count())
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
    // So `compute_layout` can re-run when only the space changed.
    last_space: FxHashMap<NodeId, (AvailableSpace, AvailableSpace)>,
    // Whether each compute-root was authored with an auto width and height, captured on its first compute, before filling the space pins them.
    root_auto: FxHashMap<NodeId, (bool, bool)>,
    overlay_host: OverlayHost,
    // Captured during the top-level root's walk, which runs from the window origin. Node rect signals stay root-local, so this map is the one place with window-absolute positions — which is what lets `absolute_rect` anchor a portaled overlay to a trigger in a sub-root.
    abs_pos: FxHashMap<NodeId, (f32, f32)>,
    /// The observable half of `abs_pos`, minted on first `absolute_rect` and never eagerly.
    ///
    /// Lazy is a constraint, not a preference: `new_leaf` already creates a rect signal per node, so minting a second one per node would double the reactive arena and its subscription bookkeeping for a value almost nothing reads. Absolute position is what a portalled overlay anchors to — a handful of nodes.
    abs_pos_signals: FxHashMap<NodeId, RwSignal<(f32, f32)>>,
    /// How many times the engine has laid anything out, for [`layout_passes`].
    passes: u64,
    // Guards against a recursive `compute()`: an effect that reads a layout signal and calls `compute_layout`.
    #[cfg(debug_assertions)]
    is_computing: bool,
}

impl LayoutRuntime {
    fn new() -> Self {
        Self {
            engine: LayoutEngine::new(),
            registry: FxHashMap::default(),
            last_space: FxHashMap::default(),
            root_auto: FxHashMap::default(),
            overlay_host: OverlayHost::default(),
            abs_pos: FxHashMap::default(),
            abs_pos_signals: FxHashMap::default(),
            passes: 0,
            #[cfg(debug_assertions)]
            is_computing: false,
        }
    }

    pub(crate) fn new_leaf(
        &mut self,
        style: LayoutStyle,
    ) -> Result<(NodeId, RwSignal<Rect>), LayoutError> {
        let node = self.engine.new_leaf(style)?;
        let signal = signal(Rect::default());
        self.registry.insert(node, signal);
        Ok((node, signal))
    }

    pub(crate) fn new_measured_leaf(
        &mut self,
        style: LayoutStyle,
        measure: MeasureFn,
    ) -> Result<(NodeId, RwSignal<Rect>), LayoutError> {
        let node = self.engine.new_measured_leaf(style, measure)?;
        let signal = signal(Rect::default());
        self.registry.insert(node, signal);
        Ok((node, signal))
    }

    pub(crate) fn new_container(
        &mut self,
        style: LayoutStyle,
        children: &[NodeId],
    ) -> Result<NodeId, LayoutError> {
        let node = self.engine.new_container(style, children)?;
        let signal = signal(Rect::default());
        self.registry.insert(node, signal);
        for &child in children {
            link_parent(child, node);
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
        // Clean here only when the root was freed: nothing above can dirty a node that is gone.
        if !self.engine.is_dirty(root) {
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
        self.engine.compute_layout(root, width, height)?;
        self.passes += 1;
        // Collected under the runtime borrow but applied only after the caller releases it: a `set` flushes effects, one of which may re-enter the runtime.
        let mut updates: Vec<Update> = Vec::new();
        // Only a parent-less root runs from the window origin, so only then are the walked rects window-absolute.
        let is_window_walk = with_parents_ref(|p| !p.contains_key(&root));
        let mut abs_updates: Vec<(NodeId, f32, f32)> = Vec::new();
        let registry = &self.registry;
        let walk_result = self.engine.walk(root, &mut |node_id, rect| {
            if let Some(sig) = registry.get(&node_id)
                && sig.peek() != rect
            {
                updates.push(Update::Rect(*sig, rect));
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

    /// Frees `node` and every node beneath it: taffy's own `remove` orphans the children, and nothing else would ever free them.
    fn remove_node(&mut self, node: NodeId) {
        let mut freed = FxHashSet::default();
        let mut pending = vec![node];
        while let Some(at) = pending.pop() {
            if !freed.insert(at) {
                continue;
            }
            pending.extend(self.engine.children(at));
            self.engine.remove(at);
            self.registry.remove(&at);
            self.last_space.remove(&at);
            self.root_auto.remove(&at);
            self.abs_pos.remove(&at);
            self.abs_pos_signals.remove(&at);
        }
        // Every link naming a freed node goes with it, not only the one it owned: taffy hands a freed index back out.
        with_parents(|p| p.retain(|below, above| !freed.contains(below) && !freed.contains(above)));
    }
}

#[cfg(test)]
#[path = "context_test.rs"]
mod tests;

/// The CSS for what `node` was asked for, resolved against the direction in force and the active surface's size.
///
/// For a backend whose output is a document rather than pixels: it is handed the box's intent, not the rect the intent produced, so the browser can lay the box out itself. `None` for a node this runtime does not own — a widget mid-teardown, or one built on another surface.
///
/// Subscribes the caller to the surface's size when the style is written as a fraction of it: those lengths reach the document as pixels, which go stale on a resize even where the box's own rect happens not to move.
pub fn declared_css(node: NodeId) -> Option<layout_core::Css> {
    let direction = crate::direction::current_direction();
    let surface = crate::surface_size::surface_size();
    let (css, follows_surface) = with_runtime(|rt| {
        rt.engine.declared_style(node).map(|style| {
            (
                style.to_css(direction, surface),
                style.is_surface_relative(),
            )
        })
    })?;
    if follows_surface {
        crate::surface_size::use_surface_size();
    }
    Some(css)
}
