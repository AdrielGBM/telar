use std::rc::Rc;

use layout_core::{LayoutError, LayoutStyle, NodeId};
use platform_core::Event;
use reactive_core::ReadSignal;
use reactive_tree::{Component, EventResult, View};
use renderer_core::{DrawCommand, TextStyle};

use crate::context::WidgetCtx;

pub struct Label {
    text: Rc<str>,
    style: TextStyle,
    layout_node: NodeId,
    rect: ReadSignal<renderer_core::Rect>,
}

impl Label {
    pub fn new(
        text: impl Into<String>,
        style: TextStyle,
        ctx: &mut WidgetCtx,
    ) -> Result<Self, LayoutError> {
        Self::build(text, LayoutStyle::new(), style, ctx)
    }

    pub fn with_size(
        text: impl Into<String>,
        width: f32,
        height: f32,
        style: TextStyle,
        ctx: &mut WidgetCtx,
    ) -> Result<Self, LayoutError> {
        Self::build(
            text,
            LayoutStyle::new().width(width).height(height),
            style,
            ctx,
        )
    }

    fn build(
        text: impl Into<String>,
        layout_style: LayoutStyle,
        style: TextStyle,
        ctx: &mut WidgetCtx,
    ) -> Result<Self, LayoutError> {
        let (node, rect) = ctx.register_leaf(layout_style)?;
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
    use renderer_core::{Color, DrawCommand, TextStyle};

    use super::*;
    use crate::context::WidgetCtx;

    #[test]
    fn label_view_returns_text_command() {
        let mut ctx = WidgetCtx::new();
        let label = Label::new("Hello", TextStyle::new(16.0, Color::WHITE), &mut ctx).unwrap();
        let view = label.view();
        assert!(matches!(view, View::Primitive(DrawCommand::Text { .. })));
    }

    #[test]
    fn label_view_reacts_to_rect_change() {
        let mut ctx = WidgetCtx::new();
        let label = Label::with_size(
            "Hi",
            120.0,
            40.0,
            TextStyle::new(14.0, Color::BLACK),
            &mut ctx,
        )
        .unwrap();
        let root = ctx
            .new_container(
                layout_core::LayoutStyle::new()
                    .flex_row()
                    .width(200.0)
                    .height(100.0),
                &[label.layout_node()],
            )
            .unwrap();
        ctx.compute(root, 200.0, 100.0).unwrap();

        let view = label.view();
        if let View::Primitive(DrawCommand::Text { rect, .. }) = view {
            assert_eq!(rect.w, 120.0);
            assert_eq!(rect.h, 40.0);
        } else {
            panic!("expected Text command");
        }
    }
}
