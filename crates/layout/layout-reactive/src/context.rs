use geometry_core::Rect;
use layout_core::{AvailableSpace, LayoutEngine, LayoutError, LayoutStyle, MeasureFn, NodeId};
use reactive_core::{RwSignal, batch, create_rw_signal};
use rustc_hash::FxHashMap;

pub fn new_leaf(
    ctx: &mut WidgetCtx,
    style: LayoutStyle,
) -> Result<(NodeId, RwSignal<Rect>), LayoutError> {
    ctx.new_leaf(style)
}

/// A leaf whose intrinsic size is computed by `measure` at layout time (e.g. text
/// whose height depends on how many lines it wraps into at the resolved width).
pub fn new_measured_leaf(
    ctx: &mut WidgetCtx,
    style: LayoutStyle,
    measure: MeasureFn,
) -> Result<(NodeId, RwSignal<Rect>), LayoutError> {
    ctx.new_measured_leaf(style, measure)
}

pub fn new_container(
    ctx: &mut WidgetCtx,
    style: LayoutStyle,
    children: &[NodeId],
) -> Result<NodeId, LayoutError> {
    ctx.new_container(style, children)
}

pub fn compute_layout(
    ctx: &mut WidgetCtx,
    root: NodeId,
    width: AvailableSpace,
    height: AvailableSpace,
) -> Result<(), LayoutError> {
    ctx.compute_layout(root, width, height)
}

pub fn track_layout(ctx: &WidgetCtx, node: NodeId) -> Option<RwSignal<Rect>> {
    ctx.track_layout(node)
}

pub fn mark_dirty(ctx: &mut WidgetCtx, node: NodeId) -> Result<(), LayoutError> {
    ctx.mark_dirty(node)
}

pub struct WidgetCtx {
    engine: LayoutEngine,
    registry: FxHashMap<NodeId, RwSignal<Rect>>,
    parents: FxHashMap<NodeId, NodeId>,
    boundary_nodes: FxHashMap<NodeId, (f32, f32)>,
    // Available space each root was last computed against, so compute_layout can
    // re-run when only the space changed (e.g. a window resize) even though the
    // node itself is clean. Without this, resizing an independently-computed root
    // is silently a no-op and its layout freezes at the first size.
    last_space: FxHashMap<NodeId, (AvailableSpace, AvailableSpace)>,
    // Nodes with a definite `max-width` and their original style. taffy sizes a
    // max-width box's intrinsic height at its uncapped width, so a wrapping child
    // reports a 1-line height and the box ends up too short. compute_layout pins
    // each resolved width as a definite width and re-runs so heights are correct.
    constrained: Vec<(NodeId, LayoutStyle)>,
    // Whether each compute-root's width/height were originally `auto`, captured the
    // first time it is computed. An auto-sized root fills the definite space it is
    // computed in, so a top-level page need not declare width:100% to avoid
    // collapsing to its content width.
    root_auto: FxHashMap<NodeId, (bool, bool)>,
    // Guards against recursive compute(): an effect that reads a layout signal and calls compute_layout() again creates a re-layout cycle caught immediately in debug builds.
    #[cfg(debug_assertions)]
    is_computing: bool,
}

impl WidgetCtx {
    pub fn new() -> Self {
        Self {
            engine: LayoutEngine::new(),
            registry: FxHashMap::default(),
            parents: FxHashMap::default(),
            boundary_nodes: FxHashMap::default(),
            last_space: FxHashMap::default(),
            constrained: Vec::new(),
            root_auto: FxHashMap::default(),
            #[cfg(debug_assertions)]
            is_computing: false,
        }
    }

    fn track_constrained(&mut self, node: NodeId, style: &LayoutStyle) {
        if style.max_width_px().is_some() {
            self.constrained.push((node, style.clone()));
        }
    }

    pub(crate) fn new_leaf(
        &mut self,
        style: LayoutStyle,
    ) -> Result<(NodeId, RwSignal<Rect>), LayoutError> {
        let node = self.engine.new_leaf(style.clone())?;
        let signal = create_rw_signal(Rect::default());
        self.registry.insert(node, signal.clone());
        if let Some(dims) = self.engine.is_fixed_size_node(node) {
            self.boundary_nodes.insert(node, dims);
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
        let signal = create_rw_signal(Rect::default());
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
        let signal = create_rw_signal(Rect::default());
        self.registry.insert(node, signal);
        for &child in children {
            self.parents.insert(child, node);
        }
        if let Some(dims) = self.engine.is_fixed_size_node(node) {
            self.boundary_nodes.insert(node, dims);
        }
        self.track_constrained(node, &style);
        Ok(node)
    }

    pub(crate) fn compute_layout(
        &mut self,
        root: NodeId,
        width: AvailableSpace,
        height: AvailableSpace,
    ) -> Result<(), LayoutError> {
        // A changed available space (window resize) must re-run layout even when the
        // node is clean: dirty the root so the cached size from the previous space is
        // discarded. Skip only when both the node is clean and the space is unchanged.
        let space_changed = self.last_space.get(&root) != Some(&(width, height));
        if space_changed {
            self.engine.mark_dirty(root).ok();
            self.last_space.insert(root, (width, height));
        } else if !self.engine.is_dirty(root) {
            return Ok(());
        }
        // A layout root fills the definite space it is computed in: an auto width or
        // height becomes the available size, so a top-level page need not declare
        // width:100% to avoid collapsing to its content. Only this root is affected.
        let (w_auto, h_auto) = match self.root_auto.get(&root).copied() {
            Some(v) => v,
            None => {
                let v = self.engine.size_is_auto(root);
                self.root_auto.insert(root, v);
                v
            }
        };
        let mut filled_root = false;
        if w_auto {
            let w = match width {
                AvailableSpace::Definite(w) => Some(w),
                _ => None,
            };
            self.engine.set_width(root, w);
            filled_root = true;
        }
        if h_auto {
            let h = match height {
                AvailableSpace::Definite(h) => Some(h),
                _ => None,
            };
            self.engine.set_height(root, h);
            filled_root = true;
        }
        if filled_root {
            self.engine.mark_dirty(root).ok();
        }
        // Undo any width pins from a previous layout so each max-width box resolves
        // against the new available space before we re-pin it after the first pass.
        for i in 0..self.constrained.len() {
            let node = self.constrained[i].0;
            let style = self.constrained[i].1.clone();
            self.engine.set_style(node, style).ok();
            self.engine.mark_dirty(node).ok();
        }
        let mut dirty_nodes = Vec::new();
        self.engine.collect_dirty_nodes(root, &mut dirty_nodes);
        if dirty_nodes.is_empty() {
            return Ok(());
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
        // Second pass: pin each max-width box to the width it just resolved to, so a
        // re-layout sizes its wrapping children at the capped width (correct line
        // count / height) instead of taffy's uncapped 1-line intrinsic estimate.
        let mut pinned_any = false;
        for i in 0..self.constrained.len() {
            let node = self.constrained[i].0;
            let style = self.constrained[i].1.clone();
            let Some(max_w) = style.max_width_px() else {
                continue;
            };
            if !self.is_in_subtree(node, layout_root) {
                continue;
            }
            if let Ok(laid) = self.engine.get_layout(node) {
                if laid.width > 0.0 && laid.width <= max_w + 0.5 {
                    self.engine.set_style(node, style.width(laid.width)).ok();
                    self.engine.mark_dirty(node).ok();
                    pinned_any = true;
                }
            }
        }
        if pinned_any {
            self.engine
                .compute_layout(layout_root, layout_width, layout_height)?;
        }
        let registry = &self.registry;
        let mut walk_result = Ok(());
        batch(|| {
            walk_result = self.engine.walk(layout_root, &mut |node_id, rect| {
                if let Some(sig) = registry.get(&node_id) {
                    if sig.peek() != rect {
                        sig.set(rect);
                    }
                }
                true
            });
        });
        #[cfg(debug_assertions)]
        {
            self.is_computing = false;
        }
        walk_result
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
            Some((boundary, bw, bh))
                if dirty_nodes.iter().all(|&n| self.is_in_subtree(n, boundary)) =>
            {
                (
                    boundary,
                    AvailableSpace::Definite(bw),
                    AvailableSpace::Definite(bh),
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
}

impl Default for WidgetCtx {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use geometry_core::Rect;
    use layout_core::{LayoutStyle, SizeDimension};

    use super::*;

    // A flex-wrap row nested in a max-width box (the full-bleed-band + centered-
    // content pattern) must reserve height for the lines it actually wraps into,
    // even though taffy would otherwise size the box at its uncapped 1-line width.
    #[test]
    fn maxwidth_box_reserves_height_for_wrapped_content() {
        let mut ctx = WidgetCtx::new();
        let mut items = Vec::new();
        for _ in 0..4 {
            let (n, _) = new_leaf(
                &mut ctx,
                LayoutStyle::new()
                    .width(200.0)
                    .height(100.0)
                    .min_width(200.0)
                    .flex_grow(1.0),
            )
            .unwrap();
            items.push(n);
        }
        let row = new_container(
            &mut ctx,
            LayoutStyle::new().flex_row().flex_wrap().gap(24.0),
            &items,
        )
        .unwrap();
        // Capped to 500 → 2 items per row → the 4 items wrap onto 2 lines.
        let boxed = new_container(
            &mut ctx,
            LayoutStyle::new()
                .flex_column()
                .width(SizeDimension::Percent(1.0))
                .max_width(500.0),
            &[row],
        )
        .unwrap();
        let page = new_container(
            &mut ctx,
            LayoutStyle::new()
                .flex_column()
                .width(SizeDimension::Percent(1.0)),
            &[boxed],
        )
        .unwrap();
        compute_layout(
            &mut ctx,
            page,
            AvailableSpace::Definite(900.0),
            AvailableSpace::MaxContent,
        )
        .unwrap();
        let box_rect = track_layout(&ctx, boxed).unwrap().get();
        let row_rect = track_layout(&ctx, row).unwrap().get();
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

    // An auto-sized layout root fills the definite space it is computed in, so a
    // page need not declare width:100% to avoid collapsing to its content width.
    #[test]
    fn auto_root_fills_definite_width() {
        let mut ctx = WidgetCtx::new();
        let (child, _) = new_leaf(&mut ctx, LayoutStyle::new().height(40.0)).unwrap();
        // A column with auto width whose child is content-sized would otherwise
        // shrink to the child; the root-fill rule stretches it to the given width.
        let page = new_container(&mut ctx, LayoutStyle::new().flex_column(), &[child]).unwrap();
        compute_layout(
            &mut ctx,
            page,
            AvailableSpace::Definite(1000.0),
            AvailableSpace::MaxContent,
        )
        .unwrap();
        let w = track_layout(&ctx, page).unwrap().get().width;
        assert!(
            (w - 1000.0).abs() < 1.0,
            "auto root did not fill width: {w}"
        );
    }

    #[test]
    fn ctx_register_leaf_returns_ok() {
        let mut ctx = WidgetCtx::new();
        let result = new_leaf(&mut ctx, LayoutStyle::new());
        assert!(result.is_ok());
    }

    #[test]
    fn ctx_new_container_returns_ok() {
        let mut ctx = WidgetCtx::new();
        let leaf_result = new_leaf(&mut ctx, LayoutStyle::new());
        assert!(leaf_result.is_ok());
        let (leaf, _) = leaf_result.unwrap();
        let container_result = new_container(&mut ctx, LayoutStyle::new(), &[leaf]);
        assert!(container_result.is_ok());
    }

    #[test]
    fn ctx_register_leaf_returns_zero_rect() {
        let mut ctx = WidgetCtx::new();
        let (_node, rect) = new_leaf(&mut ctx, LayoutStyle::new()).unwrap();
        assert_eq!(rect.get(), Rect::default());
    }

    #[test]
    fn ctx_compute_updates_rect() {
        let mut ctx = WidgetCtx::new();
        let (leaf, rect) =
            new_leaf(&mut ctx, LayoutStyle::new().width(100.0).height(50.0)).unwrap();
        let root = new_container(
            &mut ctx,
            LayoutStyle::new().flex_row().width(200.0).height(100.0),
            &[leaf],
        )
        .unwrap();
        compute_layout(
            &mut ctx,
            root,
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();
        assert_eq!(rect.get().width, 100.0);
        assert_eq!(rect.get().height, 50.0);
    }
}
