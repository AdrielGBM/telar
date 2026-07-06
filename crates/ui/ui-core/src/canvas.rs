use crate::impl_leaf_widget;
use crate::layout_leaf::LayoutLeaf;
use geometry_core::Rect;
use layout_core::{LayoutError, LayoutStyle};
use platform_core::Event;
use ui_tree::{Component, EventResult, RenderNode};

pub struct Canvas {
    leaf: LayoutLeaf,
    draw: Box<dyn Fn(Rect) -> RenderNode>,
}

impl Canvas {
    pub fn new(
        ctx: &mut crate::context::WidgetCtx,
        layout_style: LayoutStyle,
        draw_fn: impl Fn(Rect) -> RenderNode + 'static,
    ) -> Result<Self, LayoutError> {
        let leaf = LayoutLeaf::register(ctx, layout_style)?;
        Ok(Self {
            leaf,
            draw: Box::new(draw_fn),
        })
    }
}

impl Canvas {
    pub fn with_intrinsic_height(
        ctx: &mut crate::context::WidgetCtx,
        height: f32,
        draw_fn: impl Fn(geometry_core::Rect) -> ui_tree::RenderNode + 'static,
    ) -> Result<Self, layout_core::LayoutError> {
        Self::new(ctx, layout_core::LayoutStyle::new().height(height), draw_fn)
    }
}

impl_leaf_widget!(Canvas);

impl Component for Canvas {
    fn view(&self) -> RenderNode {
        let r = self.leaf.rect.get();
        // A Canvas closure draws at fixed coordinates that ignore the layout rect, so a collapsed rect
        // (e.g. a section hidden via `display:none`) would still paint over other content. Draw nothing.
        if r.width <= 0.0 || r.height <= 0.0 {
            return RenderNode::Empty;
        }
        // The closure draws in local space (at_layout_position translates the output), so it gets a zero-origin rect — passing the absolute layout rect would double-offset anything derived from rect.x/y.
        let local = Rect {
            x: 0.0,
            y: 0.0,
            width: r.width,
            height: r.height,
        };
        let inner = (self.draw)(local);
        self.leaf.at_layout_position(inner)
    }

    fn on_event(&mut self, _event: &Event) -> EventResult {
        EventResult::Ignored
    }

    fn debug_name(&self) -> &'static str {
        "Canvas"
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use layout_core::AvailableSpace;
    use renderer_core::{Color, DrawCommand, Paint, RectStyle, ShapeStyle};

    use super::*;
    use crate::context::{WidgetCtx, compute_layout, new_container};
    use crate::layout_item::LayoutItem;

    // `draw` must be re-invoked on every `view()`, not cached from construction — a `$signal` colour
    // read inside it (the reactive path the transpiler now clones into a canvas child's `fill`/`stroke`)
    // would otherwise freeze at whatever value was current when the closure was built.
    #[test]
    fn draw_closure_is_re_read_each_view_and_recolors() {
        let color = Rc::new(Cell::new(Color::RED));
        let color_read = color.clone();
        let mut ctx = WidgetCtx::new();
        let canvas = Canvas::new(
            &mut ctx,
            LayoutStyle::new().width(40.0).height(40.0),
            move |r| RenderNode::rect(r, RectStyle::default().with_fill(color_read.get())),
        )
        .unwrap();
        let root = new_container(
            &mut ctx,
            LayoutStyle::new().width(40.0).height(40.0),
            &[canvas.layout_node()],
        )
        .unwrap();
        compute_layout(
            &mut ctx,
            root,
            AvailableSpace::Definite(40.0),
            AvailableSpace::Definite(40.0),
        )
        .unwrap();

        assert_eq!(fill_of(&canvas.view()), Paint::Solid(Color::RED));
        color.set(Color::BLUE);
        assert_eq!(
            fill_of(&canvas.view()),
            Paint::Solid(Color::BLUE),
            "draw closure must be re-read on the second view(), not cached from construction"
        );
    }

    fn fill_of(view: &RenderNode) -> Paint {
        let RenderNode::Transform { children, .. } = view else {
            panic!("expected Transform")
        };
        let RenderNode::Primitive(DrawCommand::Rect { style, .. }) = &children[0] else {
            panic!("expected a Rect primitive")
        };
        style.fill.expect("expected a fill")
    }
}
