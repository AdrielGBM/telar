use layout_core::{LayoutEngine, LayoutStyle, NodeId};
use reactive_core::{ReadSignal, RwSignal, create_rw_signal};
use renderer_core::Rect;

pub struct WidgetCtx {
    engine: LayoutEngine,
    registry: Vec<(NodeId, RwSignal<Rect>)>,
}

impl WidgetCtx {
    pub fn new() -> Self {
        Self {
            engine: LayoutEngine::new(),
            registry: Vec::new(),
        }
    }

    pub fn register_leaf(&mut self, style: LayoutStyle) -> (NodeId, ReadSignal<Rect>) {
        let node = self.engine.new_leaf(style).unwrap();
        let signal = create_rw_signal(Rect::default());
        self.registry.push((node, signal.clone()));
        (node, signal.read_only())
    }

    pub fn new_container(&mut self, style: LayoutStyle, children: &[NodeId]) -> NodeId {
        self.engine.new_container(style, children).unwrap()
    }

    pub fn compute(&mut self, root: NodeId, width: f32, height: f32) {
        self.engine.compute(root, width, height).unwrap();
        let registry = &self.registry;
        self.engine
            .walk(root, &mut |node_id, rect| {
                for (id, sig) in registry {
                    if *id == node_id {
                        sig.set(rect);
                        break;
                    }
                }
            })
            .unwrap();
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
        let (_node, rect) = ctx.register_leaf(LayoutStyle::new());
        assert_eq!(rect.get(), Rect::default());
    }

    #[test]
    fn ctx_compute_updates_rect() {
        let mut ctx = WidgetCtx::new();
        let (leaf, rect) = ctx.register_leaf(LayoutStyle::new().width(100.0).height(50.0));
        let root = ctx.new_container(
            LayoutStyle::new().flex_row().width(200.0).height(100.0),
            &[leaf],
        );
        ctx.compute(root, 200.0, 100.0);
        assert_eq!(rect.get().w, 100.0);
        assert_eq!(rect.get().h, 50.0);
    }
}
