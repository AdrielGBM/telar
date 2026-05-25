use geometry_core::Rect;
use layout_core::{LayoutError, LayoutStyle, NodeId};
use platform_core::Event;
use reactive_core::RwSignal;
use ui_tree::{Component, EventResult, View};

use crate::context::{WidgetCtx, new_container, track_layout};
use crate::layout_item::LayoutItem;

pub struct Container {
    node: NodeId,
    children: Vec<(Box<dyn LayoutItem>, Option<RwSignal<Rect>>)>,
}

impl Container {
    pub fn new(
        ctx: &mut WidgetCtx,
        style: LayoutStyle,
        children: Vec<Box<dyn LayoutItem>>,
    ) -> Result<Self, LayoutError> {
        let child_nodes = children.iter().map(|c| c.layout_node()).collect::<Vec<_>>();
        let node = new_container(ctx, style, &child_nodes)?;
        let children = children
            .into_iter()
            .map(|c| {
                let rect = track_layout(ctx, c.layout_node());
                (c, rect)
            })
            .collect();
        Ok(Container { node, children })
    }

    pub fn row(
        ctx: &mut WidgetCtx,
        children: Vec<Box<dyn LayoutItem>>,
    ) -> Result<Self, LayoutError> {
        Self::new(ctx, LayoutStyle::new().flex_row(), children)
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
    fn view(&self) -> View {
        View::group(self.children.iter().map(|(c, _)| c.view()))
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        let pointer_pos: Option<(f32, f32)> = match event {
            Event::PointerMoved { x, y, .. } => Some((*x as f32, *y as f32)),
            Event::PointerPressed { x, y, .. } => Some((*x as f32, *y as f32)),
            Event::PointerReleased { x, y, .. } => Some((*x as f32, *y as f32)),
            _ => None,
        };
        for (child, rect_signal) in &mut self.children {
            if let Some((px, py)) = pointer_pos {
                if let Some(sig) = rect_signal {
                    if !sig.get().contains(px, py) {
                        continue;
                    }
                }
            }
            if child.on_event(event) == EventResult::Handled {
                return EventResult::Handled;
            }
        }
        EventResult::Ignored
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
        let container = Container::row(&mut ctx, vec![Box::new(text_a), Box::new(text_b)]).unwrap();
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
        let result = Container::row(&mut ctx, vec![]);
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
        if let View::Group(children) = view {
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
        let container = Container::row(&mut ctx, vec![]).unwrap();
        let node = container.layout_node();
        let _root = new_container(&mut ctx, LayoutStyle::new().flex_row(), &[node])
            .expect("should register");
    }

    #[test]
    fn container_can_be_nested_as_layout_item() {
        let mut ctx = WidgetCtx::new();
        let inner = Container::column(&mut ctx, vec![]).unwrap();
        let outer = Container::row(&mut ctx, vec![Box::new(inner)]);
        assert!(outer.is_ok());
    }
}
