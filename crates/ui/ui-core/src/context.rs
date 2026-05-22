use std::cell::RefCell;
use std::collections::HashMap;

use geometry_core::Rect;
use layout_core::{AvailableSpace, LayoutEngine, LayoutError, LayoutStyle, NodeId};
use reactive_core::{ReadSignal, RwSignal, batch, create_rw_signal};

thread_local! {
    static CURRENT_CTX: RefCell<Option<WidgetCtx>> = const { RefCell::new(None) };
}

/// Runs `f` with `ctx` as the active layout context on this thread. Returns the closure result and the context back.
pub fn with_context<R>(ctx: WidgetCtx, f: impl FnOnce() -> R) -> (R, WidgetCtx) {
    CURRENT_CTX.with(|c| {
        assert!(
            c.borrow().is_none(),
            "nested with_context calls are not supported"
        );
        *c.borrow_mut() = Some(ctx);
    });
    let result = f();
    let ctx = CURRENT_CTX.with(|c| {
        c.borrow_mut()
            .take()
            .expect("WidgetCtx was taken during with_context closure")
    });
    (result, ctx)
}

pub fn register_leaf(style: LayoutStyle) -> Result<(NodeId, ReadSignal<Rect>), LayoutError> {
    CURRENT_CTX.with(|c| {
        c.borrow_mut()
            .as_mut()
            .expect("no active WidgetCtx — call within with_context()")
            .register_leaf(style)
    })
}

pub fn new_container(style: LayoutStyle, children: &[NodeId]) -> Result<NodeId, LayoutError> {
    CURRENT_CTX.with(|c| {
        c.borrow_mut()
            .as_mut()
            .expect("no active WidgetCtx — call within with_context()")
            .new_container(style, children)
    })
}

pub fn compute_layout(
    root: NodeId,
    width: AvailableSpace,
    height: AvailableSpace,
) -> Result<(), LayoutError> {
    CURRENT_CTX.with(|c| {
        c.borrow_mut()
            .as_mut()
            .expect("no active WidgetCtx — call within with_context()")
            .compute(root, width, height)
    })
}

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

    pub fn compute(
        &mut self,
        root: NodeId,
        width: AvailableSpace,
        height: AvailableSpace,
    ) -> Result<(), LayoutError> {
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
        with_context(WidgetCtx::new(), || {
            let result = register_leaf(LayoutStyle::new());
            assert!(result.is_ok());
        });
    }

    #[test]
    fn ctx_new_container_returns_ok() {
        with_context(WidgetCtx::new(), || {
            let leaf_result = register_leaf(LayoutStyle::new());
            assert!(leaf_result.is_ok());
            let (leaf, _) = leaf_result.unwrap();
            let container_result = new_container(LayoutStyle::new(), &[leaf]);
            assert!(container_result.is_ok());
        });
    }

    #[test]
    fn ctx_register_leaf_returns_zero_rect() {
        with_context(WidgetCtx::new(), || {
            let (_node, rect) = register_leaf(LayoutStyle::new()).unwrap();
            assert_eq!(rect.get(), Rect::default());
        });
    }

    #[test]
    fn ctx_compute_updates_rect() {
        with_context(WidgetCtx::new(), || {
            let (leaf, rect) = register_leaf(LayoutStyle::new().width(100.0).height(50.0)).unwrap();
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
        });
    }
}
