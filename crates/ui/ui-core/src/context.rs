use std::collections::HashMap;

use layout_core::{LayoutEngine, LayoutError, LayoutStyle, NodeId};
use reactive_core::{ReadSignal, RwSignal, batch, create_rw_signal};
use renderer_core::Rect;

pub struct WidgetCtx {
    engine: LayoutEngine,
    registry: HashMap<NodeId, RwSignal<Rect>>,
}

impl WidgetCtx {
    pub fn new() -> Self {
        Self {
            engine: LayoutEngine::new(),
            registry: HashMap::new(),
        }
    }

    pub fn register_leaf(
        &mut self,
        style: LayoutStyle,
    ) -> Result<(NodeId, ReadSignal<Rect>), LayoutError> {
        let node = self.engine.new_leaf(style)?;
        let signal = create_rw_signal(Rect::default());
        self.registry.insert(node, signal.clone());
        Ok((node, signal.read_only()))
    }

    pub fn new_container(
        &mut self,
        style: LayoutStyle,
        children: &[NodeId],
    ) -> Result<NodeId, LayoutError> {
        self.engine.new_container(style, children)
    }

    pub fn compute(&mut self, root: NodeId, width: f32, height: f32) -> Result<(), LayoutError> {
        self.engine.compute(root, width, height)?;
        let registry = &self.registry;
        let mut walk_result = Ok(());
        batch(|| {
            walk_result = self.engine.walk(root, &mut |node_id, rect| {
                if let Some(sig) = registry.get(&node_id) {
                    sig.set(rect);
                }
            });
        });
        walk_result
    }

    pub fn engine(&self) -> &LayoutEngine {
        &self.engine
    }

    pub fn engine_mut(&mut self) -> &mut LayoutEngine {
        &mut self.engine
    }
}

impl Default for WidgetCtx {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use layout_core::LayoutStyle;
    use renderer_core::Rect;

    use super::*;

    #[test]
    fn ctx_register_leaf_returns_zero_rect() {
        let mut ctx = WidgetCtx::new();
        let (_node, rect) = ctx.register_leaf(LayoutStyle::new()).unwrap();
        assert_eq!(rect.get(), Rect::default());
    }

    #[test]
    fn ctx_compute_updates_rect() {
        let mut ctx = WidgetCtx::new();
        let (leaf, rect) = ctx
            .register_leaf(LayoutStyle::new().width(100.0).height(50.0))
            .unwrap();
        let root = ctx
            .new_container(
                LayoutStyle::new().flex_row().width(200.0).height(100.0),
                &[leaf],
            )
            .unwrap();
        ctx.compute(root, 200.0, 100.0).unwrap();
        assert_eq!(rect.get().w, 100.0);
        assert_eq!(rect.get().h, 50.0);
    }
}
