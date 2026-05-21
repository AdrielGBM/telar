use layout_core::{LayoutError, LayoutStyle, NodeId};
use platform_core::Event;
use ui_tree::{Component, EventResult, View};

use crate::context::new_container;
use crate::layout_item::LayoutItem;

pub struct FlexContainer {
    node: NodeId,
    children: Vec<Box<dyn LayoutItem>>,
}

impl FlexContainer {
    pub fn new(
        style: LayoutStyle,
        children: Vec<Box<dyn LayoutItem>>,
    ) -> Result<Self, LayoutError> {
        let child_nodes = children.iter().map(|c| c.layout_node()).collect::<Vec<_>>();
        let node = new_container(style, &child_nodes)?;
        Ok(FlexContainer { node, children })
    }

    pub fn row(children: Vec<Box<dyn LayoutItem>>) -> Result<Self, LayoutError> {
        Self::new(LayoutStyle::new().flex_row(), children)
    }

    pub fn column(children: Vec<Box<dyn LayoutItem>>) -> Result<Self, LayoutError> {
        Self::new(LayoutStyle::new().flex_column(), children)
    }
}

impl LayoutItem for FlexContainer {
    fn layout_node(&self) -> NodeId {
        self.node
    }
}

impl Component for FlexContainer {
    fn view(&self) -> View {
        let child_views: Vec<View> = self.children.iter().map(|c| c.view()).collect();
        View::group(child_views)
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        for child in &mut self.children {
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
    use crate::context::{WidgetCtx, compute_layout, new_container, with_context};
    use crate::label::Label;

    fn make_container_with_labels() -> (FlexContainer, WidgetCtx) {
        with_context(WidgetCtx::new(), || {
            let text_style = TextStyle::new(14.0, Color::WHITE);
            let label_a = Label::new(
                || "A".to_string(),
                LayoutStyle::new().width(50.0).height(20.0),
                text_style,
            )
            .unwrap();
            let label_b = Label::new(
                || "B".to_string(),
                LayoutStyle::new().width(50.0).height(20.0),
                text_style,
            )
            .unwrap();
            let container = FlexContainer::row(vec![Box::new(label_a), Box::new(label_b)]).unwrap();
            let root = new_container(
                LayoutStyle::new().flex_row().width(200.0).height(100.0),
                &[container.layout_node()],
            )
            .unwrap();
            compute_layout(
                root,
                AvailableSpace::Definite(200.0),
                AvailableSpace::Definite(100.0),
            )
            .unwrap();
            container
        })
    }

    #[test]
    fn flex_container_row_creates_ok() {
        with_context(WidgetCtx::new(), || {
            let result = FlexContainer::row(vec![]);
            assert!(result.is_ok());
        });
    }

    #[test]
    fn flex_container_column_creates_ok() {
        with_context(WidgetCtx::new(), || {
            let result = FlexContainer::column(vec![]);
            assert!(result.is_ok());
        });
    }

    #[test]
    fn flex_container_view_returns_group_with_children() {
        let (container, _ctx) = make_container_with_labels();
        let view = container.view();
        if let View::Group(children) = view {
            assert_eq!(children.len(), 2);
        } else {
            panic!("expected Group");
        }
    }

    #[test]
    fn flex_container_on_event_returns_ignored_with_no_handlers() {
        let (mut container, _ctx) = make_container_with_labels();
        let result = container.on_event(&Event::PointerMoved {
            x: 0.0,
            y: 0.0,
            source: PointerSource::Mouse,
        });
        assert!(matches!(result, EventResult::Ignored));
    }

    #[test]
    fn flex_container_layout_node_is_valid() {
        with_context(WidgetCtx::new(), || {
            let container = FlexContainer::row(vec![]).unwrap();
            let node = container.layout_node();
            // node must be usable as a container child without panicking
            let _root =
                new_container(LayoutStyle::new().flex_row(), &[node]).expect("should register");
        });
    }

    #[test]
    fn flex_container_can_be_nested_as_layout_item() {
        with_context(WidgetCtx::new(), || {
            let inner = FlexContainer::column(vec![]).unwrap();
            let outer = FlexContainer::row(vec![Box::new(inner)]);
            assert!(outer.is_ok());
        });
    }
}
