use std::rc::Rc;

use layout_core::{LayoutError, LayoutStyle, NodeId};
use platform_core::Event;
use renderer_core::{DrawCommand, TextStyle};
use ui_tree::{Component, EventResult, View};

use crate::layout_leaf::LayoutLeaf;

pub struct Label {
    text: Box<dyn Fn() -> Rc<str>>,
    style: TextStyle,
    leaf: LayoutLeaf,
}

impl Label {
    pub fn new(
        text: impl Fn() -> String + 'static,
        layout: LayoutStyle,
        style: TextStyle,
    ) -> Result<Self, LayoutError> {
        Self::build(move || Rc::from(text()), layout, style)
    }

    fn build(
        text: impl Fn() -> Rc<str> + 'static,
        layout: LayoutStyle,
        style: TextStyle,
    ) -> Result<Self, LayoutError> {
        let leaf = LayoutLeaf::register(layout)?;
        Ok(Self {
            text: Box::new(text),
            style,
            leaf,
        })
    }

    pub fn layout_node(&self) -> NodeId {
        self.leaf.node
    }
}

impl Component for Label {
    fn view(&self) -> View {
        let rect = self.leaf.rect.get();
        let text = (self.text)();
        View::Primitive(DrawCommand::Text {
            text,
            rect,
            style: self.style,
        })
    }

    fn on_event(&mut self, _event: &Event) -> EventResult {
        EventResult::Ignored
    }
}

#[cfg(test)]
mod tests {
    use layout_core::AvailableSpace;
    use renderer_core::{Color, DrawCommand, TextStyle};

    use super::*;
    use crate::context::{WidgetCtx, compute_layout, new_container, with_context};

    #[test]
    fn label_view_returns_text_command() {
        with_context(WidgetCtx::new(), || {
            let label = Label::new(
                || "Hello".to_string(),
                LayoutStyle::new(),
                TextStyle::new(16.0, Color::WHITE),
            )
            .unwrap();
            let view = label.view();
            assert!(matches!(view, View::Primitive(DrawCommand::Text { .. })));
        });
    }

    #[test]
    fn label_view_reacts_to_rect_change() {
        with_context(WidgetCtx::new(), || {
            let label = Label::new(
                || "Hi".to_string(),
                LayoutStyle::new().width(120.0).height(40.0),
                TextStyle::new(14.0, Color::BLACK),
            )
            .unwrap();
            let root = new_container(
                layout_core::LayoutStyle::new()
                    .flex_row()
                    .width(200.0)
                    .height(100.0),
                &[label.layout_node()],
            )
            .unwrap();
            compute_layout(
                root,
                AvailableSpace::Definite(200.0),
                AvailableSpace::Definite(100.0),
            )
            .unwrap();

            let view = label.view();
            if let View::Primitive(DrawCommand::Text { rect, .. }) = view {
                assert_eq!(rect.width, 120.0);
                assert_eq!(rect.height, 40.0);
            } else {
                panic!("expected Text command");
            }
        });
    }
}
