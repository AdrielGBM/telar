use geometry_core::Rect;
use layout_core::{LayoutError, LayoutStyle, NodeId};
use platform_core::Event;
use reactive_core::RwSignal;
use renderer_core::RectStyle;
use ui_tree::{Component, EventResult, RenderNode};

use crate::context::WidgetCtx;
use crate::layout_item::{LayoutItem, TrackedChildren, register_container};
use crate::pointer::dispatch_container_event;

pub struct StyledContainer {
    node: NodeId,
    rect: RwSignal<Rect>,
    style: Box<dyn Fn(Rect) -> RectStyle>,
    opacity: f32,
    children: TrackedChildren,
}

impl StyledContainer {
    pub fn new(
        ctx: &mut WidgetCtx,
        layout_style: LayoutStyle,
        style: impl Fn(Rect) -> RectStyle + 'static,
        children: Vec<Box<dyn LayoutItem>>,
    ) -> Result<Self, LayoutError> {
        let (node, rect, children) = register_container(ctx, layout_style, children)?;
        Ok(Self {
            node,
            rect,
            style: Box::new(style),
            opacity: 1.0,
            children,
        })
    }

    pub fn with_opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        self
    }
}

impl LayoutItem for StyledContainer {
    fn layout_node(&self) -> NodeId {
        self.node
    }
}

impl Component for StyledContainer {
    fn view(&self) -> RenderNode {
        let r = self.rect.get();
        let background = RenderNode::rect(
            Rect {
                x: r.x,
                y: r.y,
                width: r.width,
                height: r.height,
            },
            (self.style)(r),
        );
        let content = RenderNode::group(
            std::iter::once(background).chain(self.children.iter().map(|c| c.segment.boundary())),
        );
        if self.opacity < 1.0 {
            RenderNode::layer(self.opacity, 0.0, [content])
        } else {
            content
        }
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        dispatch_container_event(&mut self.children, event)
    }
}

#[cfg(test)]
mod tests {
    use layout_core::AvailableSpace;
    use platform_core::{PointerButton, PointerSource};
    use renderer_core::{Color, ShapeStyle};
    use theme_core::{Theme, WidgetTheme, set_theme_with_widgets, use_theme};

    use super::*;
    use crate::button::Button;
    use crate::container::Container;
    use crate::context::{compute_layout, track_layout};

    #[derive(Clone)]
    struct TestTheme(Color);
    impl Theme for TestTheme {
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }
    impl WidgetTheme for TestTheme {
        fn widget_primary(&self) -> Color {
            self.0
        }
        fn widget_on_primary(&self) -> Color {
            Color::WHITE
        }
    }

    // Clicking a theme button (which sets the global THEME) while a themed StyledContainer ancestor is on the dispatch stack must not re-enter that ancestor's render segment mid borrow_mut.
    #[test]
    fn theme_button_click_force_tick_no_panic() {
        set_theme_with_widgets(TestTheme(Color::RED));

        let mut ctx = WidgetCtx::new();
        let btn = Button::new(&mut ctx, "x").unwrap();
        let btn_node = btn.layout_node();
        let btn = btn.on_click(move || set_theme_with_widgets(TestTheme(Color::GREEN)));
        let inner = Container::new(
            &mut ctx,
            LayoutStyle::new().flex_column().width(200.0).height(100.0),
            vec![Box::new(btn)],
        )
        .unwrap();
        let card = StyledContainer::new(
            &mut ctx,
            LayoutStyle::new().flex_column().width(200.0).height(100.0),
            |_r| RectStyle::default().with_fill(use_theme::<TestTheme>().0),
            vec![Box::new(inner)],
        )
        .unwrap();
        let card_node = card.layout_node();
        compute_layout(
            &mut ctx,
            card_node,
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();
        let br = track_layout(&ctx, btn_node).unwrap().get();

        let mut tree = crate::ComponentList::new(card);
        let _ = tree.commands();

        reactive_core::begin_batch();
        let handled = tree.on_event(&Event::PointerPressed {
            x: (br.x + br.width / 2.0) as f64,
            y: (br.y + br.height / 2.0) as f64,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        });
        if handled == EventResult::Handled {
            tree.bump_force_ticks();
            reactive_core::end_batch();
            reactive_core::begin_batch();
        }
        let _ = tree.commands();
        reactive_core::end_batch();
    }
}
