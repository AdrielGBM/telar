use geometry_core::Rect;
use layout_core::{AvailableSpace, LayoutEngine, LayoutError, LayoutStyle, MeasureFn, NodeId};
use reactive_core::{RwSignal, batch, signal};
use rustc_hash::FxHashMap;

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

pub fn compute_layout(
    root: NodeId,
    width: AvailableSpace,
    height: AvailableSpace,
) -> Result<(), LayoutError> {
    compute_layout_root(root, width, height)
}

/// Lays out `root` against the given space and reflects the result into each node's rect signal.
/// Collects the (signal, rect) updates while holding the runtime borrow, then applies them in a batch
/// *after* releasing it — a rect `.set()` can flush effects, and one of those may itself touch the
/// layout runtime (a reactive list), which would re-enter the borrow.
pub fn compute_layout_root(
    root: NodeId,
    width: AvailableSpace,
    height: AvailableSpace,
) -> Result<(), LayoutError> {
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
        let _ = compute_layout_root(root, width, height);
    }
}

pub fn track_layout(node: NodeId) -> Option<RwSignal<Rect>> {
    with_runtime(|rt| rt.track_layout(node))
}

/// The node's WINDOW-absolute rect (top-left from the top-level walk, size from its layout), or `None` if it
/// has not been laid out under a window root yet. Unlike `track_layout`, this is correct even for a node in a
/// sub-root computed separately (whose rect signal is root-local) — use it to anchor a portaled overlay to a
/// trigger, since the portal hoists out of ancestor transforms and needs absolute coordinates. Note: it is
/// the trigger's laid-out (un-scrolled) position; a scrolled-away trigger's on-screen spot also needs the
/// scroll offset, which the layout runtime does not track (a follow-up).
pub fn absolute_rect(node: NodeId) -> Option<Rect> {
    with_runtime(|rt| {
        let &(x, y) = rt.abs_pos.get(&node)?;
        let size = rt.registry.get(&node).map(|s| s.peek()).unwrap_or_default();
        Some(Rect::new(x, y, size.width, size.height))
    })
}

pub fn mark_dirty(node: NodeId) -> Result<(), LayoutError> {
    with_runtime(|rt| rt.mark_dirty(node))
}

/// Shows or hides a node in layout flow. A hidden node takes no space (and lays out none of its subtree); mark an ancestor dirty and recompute for the change to take effect. Used for responsive layouts (e.g. collapsing a sidebar on narrow windows).
pub fn set_display(node: NodeId, visible: bool) {
    with_runtime(|rt| rt.set_display(node, visible))
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
        if rt.engine.add_child(host, node).is_err() {
            return false;
        }
        rt.parents.insert(node, host);
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
        if let Some(host) = rt.parents.remove(&node) {
            rt.engine.remove_child(host, node).ok();
            rt.engine.mark_dirty(host).ok();
        }
    });
}

struct LayoutRuntime {
    engine: LayoutEngine,
    registry: FxHashMap<NodeId, RwSignal<Rect>>,
    parents: FxHashMap<NodeId, NodeId>,
    boundary_nodes: FxHashMap<NodeId, (f32, f32)>,
    // Available space each root was last computed against, so compute_layout can re-run when only the space changed (e.g. a window resize) even though the node itself is clean. Without this, resizing an independently-computed root is silently a no-op and its layout freezes at the first size.
    last_space: FxHashMap<NodeId, (AvailableSpace, AvailableSpace)>,
    // Nodes with a definite `max-width`, their original style, and the width pinned on the previous compute (`None` = unpinned). taffy sizes a max-width box's intrinsic height at its uncapped width, so a wrapping child reports a 1-line height and the box ends up too short. compute_layout pins each resolved width as a definite width and re-runs so heights are correct. The stored pin lets the undo pass stay idempotent: an unpinned box whose space did not change is left untouched.
    constrained: Vec<(NodeId, LayoutStyle, Option<f32>)>,
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
            parents: FxHashMap::default(),
            boundary_nodes: FxHashMap::default(),
            last_space: FxHashMap::default(),
            constrained: Vec::new(),
            root_auto: FxHashMap::default(),
            overlay_host: None,
            host_pinned: false,
            abs_pos: FxHashMap::default(),
            #[cfg(debug_assertions)]
            is_computing: false,
        }
    }

    fn track_constrained(&mut self, node: NodeId, style: &LayoutStyle) {
        if style.max_width_px().is_some() {
            self.constrained.push((node, style.clone(), None));
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
        self.track_constrained(node, &style);
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
        self.track_constrained(node, &style);
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
            self.parents.insert(child, node);
        }
        if let Some(dimensions) = self.engine.is_fixed_size(node) {
            self.boundary_nodes.insert(node, dimensions);
        }
        self.track_constrained(node, &style);
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
            && !self.parents.contains_key(&root)
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
        // Undo any width pins from a previous layout so each max-width box resolves against the new available space before we re-pin it after the first pass. Idempotent: only touch a box when the space changed (everything must re-resolve) or it actually carried a pin to lift. Leaving unpinned boxes alone when the space is unchanged avoids dirtying their ancestors, which would otherwise force find_boundary_root to fall back to global_root every frame. This runs before the root-fill below so that when the root itself is a max-width box, restoring its original (auto-width) style does not clobber the definite width the fill assigns.
        for i in 0..self.constrained.len() {
            let node = self.constrained[i].0;
            let had_pin = self.constrained[i].2.is_some();
            if !is_space_changed && !had_pin {
                continue;
            }
            let style = self.constrained[i].1.clone();
            self.engine.set_style(node, style).ok();
            self.engine.mark_dirty(node).ok();
            self.constrained[i].2 = None;
        }
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
        // Second pass: pin each max-width box to the width it just resolved to, so a re-layout sizes its wrapping children at the capped width (correct line count / height) instead of taffy's uncapped 1-line intrinsic estimate.
        let mut did_pin_any = false;
        for i in 0..self.constrained.len() {
            let node = self.constrained[i].0;
            let style = self.constrained[i].1.clone();
            let Some(max_w) = style.max_width_px() else {
                continue;
            };
            if !self.is_in_subtree(node, layout_root) {
                continue;
            }
            if let Ok(layout) = self.engine.layout(node) {
                if layout.width > 0.0 && layout.width <= max_w + 0.5 {
                    self.engine.set_style(node, style.width(layout.width)).ok();
                    self.engine.mark_dirty(node).ok();
                    self.constrained[i].2 = Some(layout.width);
                    did_pin_any = true;
                }
            }
        }
        if did_pin_any {
            self.engine
                .compute_layout(layout_root, layout_width, layout_height)?;
        }
        // Collect the changed rects while holding the runtime borrow, but apply them (`sig.set`) only
        // after the caller releases it: a set flushes effects, one of which may re-enter the runtime.
        let mut updates: Vec<(RwSignal<Rect>, Rect)> = Vec::new();
        // Only a full walk of a parent-less root runs from the window origin, so only then are the walked
        // rects window-absolute. A sub-boundary or sub-root walk is root-local — don't capture those.
        let is_window_walk = layout_root == root && !self.parents.contains_key(&root);
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

    fn find_nearest_boundary(&self, mut node: NodeId) -> Option<(NodeId, f32, f32)> {
        loop {
            if let Some(&(w, h)) = self.boundary_nodes.get(&node) {
                return Some((node, w, h));
            }
            node = *self.parents.get(&node)?;
        }
    }

    fn is_in_subtree(&self, mut node: NodeId, ancestor: NodeId) -> bool {
        loop {
            if node == ancestor {
                return true;
            }
            match self.parents.get(&node) {
                Some(&parent) => node = parent,
                None => return false,
            }
        }
    }

    pub(crate) fn track_layout(&self, node: NodeId) -> Option<RwSignal<Rect>> {
        self.registry.get(&node).cloned()
    }

    pub(crate) fn mark_dirty(&mut self, node: NodeId) -> Result<(), LayoutError> {
        self.engine.mark_dirty(node)
    }

    pub(crate) fn set_display(&mut self, node: NodeId, visible: bool) {
        self.engine.set_display(node, visible);
    }

    fn set_children(&mut self, parent: NodeId, children: &[NodeId]) -> Result<(), LayoutError> {
        self.engine.set_children(parent, children)?;
        for &child in children {
            self.parents.insert(child, parent);
        }
        self.engine.mark_dirty(parent).ok();
        Ok(())
    }

    fn remove_node(&mut self, node: NodeId) {
        self.engine.remove(node);
        self.registry.remove(&node);
        self.parents.remove(&node);
        self.boundary_nodes.remove(&node);
        self.last_space.remove(&node);
        self.root_auto.remove(&node);
        self.abs_pos.remove(&node);
        self.constrained.retain(|(n, _, _)| *n != node);
    }
}

#[cfg(test)]
mod tests {
    use geometry_core::Rect;
    use layout_core::{LayoutStyle, SizeDimension};

    use super::*;

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

    // Re-running compute_layout against the SAME available space (root re-dirtied by an unrelated change) must keep the max-width box correctly sized: the idempotent undo must still lift and re-pin a previously pinned box so its wrapped height holds.
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
