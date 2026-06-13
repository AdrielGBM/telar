use geometry_core::Rect;
use layout_core::{AvailableSpace, LayoutEngine, LayoutError, LayoutStyle, NodeId};
use reactive_core::{RwSignal, batch, create_rw_signal};
use rustc_hash::FxHashMap;

pub fn new_leaf(
    ctx: &mut WidgetCtx,
    style: LayoutStyle,
) -> Result<(NodeId, RwSignal<Rect>), LayoutError> {
    ctx.new_leaf(style)
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

pub fn update_style(
    ctx: &mut WidgetCtx,
    node: NodeId,
    style: LayoutStyle,
) -> Result<(), LayoutError> {
    ctx.update_style(node, style)
}

pub fn mark_dirty(ctx: &mut WidgetCtx, node: NodeId) -> Result<(), LayoutError> {
    ctx.mark_dirty_node(node)
}

pub fn with_context<F, R>(ctx: WidgetCtx, f: F) -> (R, WidgetCtx)
where
    F: FnOnce(&mut WidgetCtx) -> R,
{
    let mut ctx = ctx;
    let result = f(&mut ctx);
    (result, ctx)
}

pub struct WidgetCtx {
    engine: LayoutEngine,
    registry: FxHashMap<NodeId, RwSignal<Rect>>,
    parents: FxHashMap<NodeId, NodeId>,
    boundary_nodes: FxHashMap<NodeId, (f32, f32)>,
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
            #[cfg(debug_assertions)]
            is_computing: false,
        }
    }

    pub fn new_leaf(
        &mut self,
        style: LayoutStyle,
    ) -> Result<(NodeId, RwSignal<Rect>), LayoutError> {
        let node = self.engine.new_leaf(style)?;
        let signal = create_rw_signal(Rect::default());
        self.registry.insert(node, signal.clone());
        if let Some(dims) = self.engine.is_fixed_size_node(node) {
            self.boundary_nodes.insert(node, dims);
        }
        Ok((node, signal))
    }

    pub fn new_container(
        &mut self,
        style: LayoutStyle,
        children: &[NodeId],
    ) -> Result<NodeId, LayoutError> {
        let node = self.engine.new_container(style, children)?;
        let signal = create_rw_signal(Rect::default());
        self.registry.insert(node, signal);
        for &child in children {
            self.parents.insert(child, node);
        }
        if let Some(dims) = self.engine.is_fixed_size_node(node) {
            self.boundary_nodes.insert(node, dims);
        }
        Ok(node)
    }

    pub fn compute_layout(
        &mut self,
        root: NodeId,
        width: AvailableSpace,
        height: AvailableSpace,
    ) -> Result<(), LayoutError> {
        if !self.engine.is_dirty(root) {
            return Ok(());
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

    pub fn track_layout(&self, node: NodeId) -> Option<RwSignal<Rect>> {
        self.registry.get(&node).cloned()
    }

    pub fn update_style(&mut self, node: NodeId, style: LayoutStyle) -> Result<(), LayoutError> {
        self.engine.set_style(node, style)?;
        match self.engine.is_fixed_size_node(node) {
            Some(dims) => {
                self.boundary_nodes.insert(node, dims);
            }
            None => {
                self.boundary_nodes.remove(&node);
            }
        }
        Ok(())
    }

    pub fn mark_dirty_node(&mut self, node: NodeId) -> Result<(), LayoutError> {
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
    use layout_core::LayoutStyle;

    use super::*;

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
