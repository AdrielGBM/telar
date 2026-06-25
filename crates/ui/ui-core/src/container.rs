use geometry_core::Rect;
use layout_core::{LayoutError, LayoutStyle, NodeId};
use platform_core::Event;
use reactive_core::RwSignal;
use ui_tree::{Component, EventResult, RenderNode};

use crate::context::WidgetCtx;
use crate::layout_item::{LayoutItem, TrackedChildren, register_container};
use crate::pointer::dispatch_container_event;

pub struct Container {
    node: NodeId,
    rect: RwSignal<Rect>,
    children: TrackedChildren,
}

impl Container {
    pub fn new(
        ctx: &mut WidgetCtx,
        layout_style: LayoutStyle,
        children: Vec<Box<dyn LayoutItem>>,
    ) -> Result<Self, LayoutError> {
        let (node, rect, children) = register_container(ctx, layout_style, children)?;
        Ok(Container {
            node,
            rect,
            children,
        })
    }

    pub fn rect(&self) -> RwSignal<Rect> {
        self.rect.clone()
    }

    pub fn column(
        ctx: &mut WidgetCtx,
        children: Vec<Box<dyn LayoutItem>>,
    ) -> Result<Self, LayoutError> {
        Self::new(ctx, LayoutStyle::new().flex_column(), children)
    }
}

impl LayoutItem for Container {
    fn layout_node(&self) -> NodeId {
        self.node
    }
}

impl Component for Container {
    fn view(&self) -> RenderNode {
        // Each child is its own segment: referencing it is a cheap Rc clone, so this view() does not re-run children and is not subscribed to their signals.
        RenderNode::group(self.children.iter().map(|c| c.segment.boundary()))
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        dispatch_container_event(&mut self.children, event)
    }
}

#[cfg(test)]
mod tests {
    use layout_core::AvailableSpace;
    use platform_core::{Event, PointerSource};
    use renderer_core::{Color, TextStyle};

    use super::*;
    use crate::context::{WidgetCtx, compute_layout, new_container};
    use crate::text::Text;

    fn make_container_with_labels() -> Container {
        let mut ctx = WidgetCtx::new();
        let text_style = TextStyle::new(14.0, Color::WHITE);
        let text_a = Text::new(
            &mut ctx,
            || "A".to_string(),
            LayoutStyle::new().width(50.0).height(20.0),
            move || text_style,
        )
        .unwrap();
        let text_b = Text::new(
            &mut ctx,
            || "B".to_string(),
            LayoutStyle::new().width(50.0).height(20.0),
            move || text_style,
        )
        .unwrap();
        let container = Container::new(
            &mut ctx,
            LayoutStyle::new().flex_row(),
            vec![Box::new(text_a), Box::new(text_b)],
        )
        .unwrap();
        let root = new_container(
            &mut ctx,
            LayoutStyle::new().flex_row().width(200.0).height(100.0),
            &[container.layout_node()],
        )
        .unwrap();
        compute_layout(
            &mut ctx,
            root,
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();
        container
    }

    #[test]
    fn container_row_creates_ok() {
        let mut ctx = WidgetCtx::new();
        let result = Container::new(&mut ctx, LayoutStyle::new().flex_row(), vec![]);
        assert!(result.is_ok());
    }

    #[test]
    fn container_column_creates_ok() {
        let mut ctx = WidgetCtx::new();
        let result = Container::column(&mut ctx, vec![]);
        assert!(result.is_ok());
    }

    #[test]
    fn container_view_returns_group_with_children() {
        let container = make_container_with_labels();
        let view = container.view();
        if let RenderNode::Group { children, .. } = view {
            assert_eq!(children.len(), 2);
        } else {
            panic!("expected Group");
        }
    }

    #[test]
    fn container_on_event_returns_ignored_with_no_handlers() {
        let mut container = make_container_with_labels();
        let result = container.on_event(&Event::PointerMoved {
            x: 0.0,
            y: 0.0,
            source: PointerSource::Mouse,
        });
        assert!(matches!(result, EventResult::Ignored));
    }

    #[test]
    fn container_layout_node_is_valid() {
        let mut ctx = WidgetCtx::new();
        let container = Container::new(&mut ctx, LayoutStyle::new().flex_row(), vec![]).unwrap();
        let node = container.layout_node();
        let _root = new_container(&mut ctx, LayoutStyle::new().flex_row(), &[node])
            .expect("should register");
    }

    #[test]
    fn click_with_force_tick_does_not_panic() {
        use crate::button::Button;
        use crate::context::track_layout;
        use platform_core::PointerButton;
        use reactive_core::{begin_batch, create_rw_signal, end_batch};

        let mut ctx = WidgetCtx::new();
        let s = create_rw_signal(0i32);
        let s_cb = s.clone();
        let btn = Button::new(&mut ctx, "x").unwrap();
        let btn_node = btn.layout_node();
        let btn = btn.on_click(move || s_cb.update(|n| *n += 1));
        let s_txt = s.clone();
        let txt = crate::text::Text::new(
            &mut ctx,
            move || format!("{}", s_txt.get()),
            LayoutStyle::new().width(50.0).height(20.0),
            || renderer_core::TextStyle::new(14.0, renderer_core::Color::BLACK),
        )
        .unwrap();
        let root = Container::new(
            &mut ctx,
            LayoutStyle::new().flex_column().width(200.0).height(100.0),
            vec![Box::new(btn), Box::new(txt)],
        )
        .unwrap();
        let root_node = root.layout_node();
        compute_layout(
            &mut ctx,
            root_node,
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();
        let br = track_layout(&ctx, btn_node).unwrap().get();

        let mut tree = crate::ComponentList::new(root);
        let _ = tree.commands();

        // Mimic the runner's event cycle, including the dev-only force-tick.
        begin_batch();
        let handled = tree.on_event(&Event::PointerPressed {
            x: (br.x + br.width / 2.0) as f64,
            y: (br.y + br.height / 2.0) as f64,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        });
        if handled == EventResult::Handled {
            tree.bump_force_ticks();
            end_batch();
            begin_batch();
        }
        let _ = tree.commands();
        end_batch();

        assert_eq!(s.get(), 1, "click should have incremented the signal");
    }

    #[test]
    fn container_can_be_nested_as_layout_item() {
        let mut ctx = WidgetCtx::new();
        let inner = Container::column(&mut ctx, vec![]).unwrap();
        let outer = Container::new(
            &mut ctx,
            LayoutStyle::new().flex_row(),
            vec![Box::new(inner)],
        );
        assert!(outer.is_ok());
    }
}
