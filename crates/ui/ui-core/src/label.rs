use std::rc::Rc;

use layout_core::{LayoutError, LayoutStyle, NodeId};
use platform_core::Event;
use reactive_core::ReadSignal;
use renderer_core::{DrawCommand, TextStyle};
use ui_tree::{Component, EventResult, View};

use crate::context;

pub struct Label {
    text: Rc<str>,
    style: TextStyle,
    layout_node: NodeId,
    rect: ReadSignal<renderer_core::Rect>,
}

impl Label {
    pub fn new(text: impl Into<String>, style: TextStyle) -> Result<Self, LayoutError> {
        Self::build(text, LayoutStyle::new(), style)
    }

    pub fn with_size(
        text: impl Into<String>,
        width: f32,
        height: f32,
        style: TextStyle,
    ) -> Result<Self, LayoutError> {
        Self::build(text, LayoutStyle::new().width(width).height(height), style)
    }

    fn build(
        text: impl Into<String>,
        layout_style: LayoutStyle,
        style: TextStyle,
    ) -> Result<Self, LayoutError> {
        let (node, rect) = context::register_leaf(layout_style)?;
        Ok(Self {
            text: Rc::from(text.into()),
            style,
            layout_node: node,
            rect,
        })
    }

    pub fn layout_node(&self) -> NodeId {
        self.layout_node
    }
}

impl Component for Label {
    fn view(&self) -> View {
        let rect = self.rect.get();
        View::Primitive(DrawCommand::Text {
            text: Rc::clone(&self.text),
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
            let label = Label::new("Hello", TextStyle::new(16.0, Color::WHITE)).unwrap();
            let view = label.view();
            assert!(matches!(view, View::Primitive(DrawCommand::Text { .. })));
        });
    }

    #[test]
    fn label_view_reacts_to_rect_change() {
        with_context(WidgetCtx::new(), || {
            let label =
                Label::with_size("Hi", 120.0, 40.0, TextStyle::new(14.0, Color::BLACK)).unwrap();
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
