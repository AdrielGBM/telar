use std::rc::Rc;

use layout_core::{LayoutStyle, NodeId};
use platform_core::{Event, PointerButton};
use reactive_core::{ReadSignal, RwSignal, create_rw_signal};
use reactive_tree::{Component, EventResult, View};
use renderer_core::{BorderRadius, Color, DrawCommand, Rect, RectStyle, TextStyle};

use crate::context::WidgetCtx;

pub struct Button {
    label: Rc<str>,
    layout_node: NodeId,
    rect: ReadSignal<Rect>,
    bg: Color,
    hover_bg: Color,
    text_style: TextStyle,
    on_click: Option<Box<dyn Fn()>>,
    hovered: RwSignal<bool>,
}

impl Button {
    pub fn new(label: impl Into<String>, ctx: &mut WidgetCtx) -> Self {
        let (node, rect) = ctx.register_leaf(LayoutStyle::new().height(36.0));
        Self {
            label: Rc::from(label.into()),
            layout_node: node,
            rect,
            bg: Color::from_rgb_u8(59, 130, 246),
            hover_bg: Color::from_rgb_u8(37, 99, 235),
            text_style: TextStyle::new(14.0, Color::WHITE),
            on_click: None,
            hovered: create_rw_signal(false),
        }
    }

    pub fn on_click(mut self, f: impl Fn() + 'static) -> Self {
        self.on_click = Some(Box::new(f));
        self
    }

    pub fn with_bg(mut self, bg: Color, hover_bg: Color) -> Self {
        self.bg = bg;
        self.hover_bg = hover_bg;
        self
    }

    pub fn layout_node(&self) -> NodeId {
        self.layout_node
    }
}

impl Component for Button {
    fn view(&self) -> View {
        let rect = self.rect.get();
        let color = if self.hovered.get() {
            self.hover_bg
        } else {
            self.bg
        };

        View::group([
            View::Primitive(DrawCommand::Rect {
                rect,
                style: RectStyle::default()
                    .with_fill(color)
                    .with_radius(BorderRadius::all(4.0)),
            }),
            View::Primitive(DrawCommand::Text {
                text: Rc::clone(&self.label),
                rect,
                style: self.text_style,
            }),
        ])
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        let rect = self.rect.get();
        match event {
            Event::PointerMoved { x, y, .. } => {
                let now = point_in_rect(*x as f32, *y as f32, rect);
                if now != self.hovered.get() {
                    self.hovered.set(now);
                    EventResult::Handled
                } else {
                    EventResult::Ignored
                }
            }
            Event::PointerPressed {
                x,
                y,
                button: PointerButton::Primary,
                ..
            } => {
                if point_in_rect(*x as f32, *y as f32, rect) {
                    if let Some(cb) = &self.on_click {
                        cb();
                    }
                    EventResult::Handled
                } else {
                    EventResult::Ignored
                }
            }
            _ => EventResult::Ignored,
        }
    }
}

fn point_in_rect(x: f32, y: f32, rect: Rect) -> bool {
    x >= rect.x && x < rect.x + rect.w && y >= rect.y && y < rect.y + rect.h
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use platform_core::{Event, PointerButton, PointerSource};
    use renderer_core::{Color, DrawCommand};

    use super::*;
    use crate::context::WidgetCtx;

    fn make_button_with_rect() -> (Button, WidgetCtx) {
        let mut ctx = WidgetCtx::new();
        let button = Button::new("OK", &mut ctx);
        let root = ctx.new_container(
            layout_core::LayoutStyle::new()
                .flex_column()
                .width(200.0)
                .height(100.0),
            &[button.layout_node()],
        );
        ctx.compute(root, 200.0, 100.0).unwrap();
        (button, ctx)
    }

    #[test]
    fn button_view_renders_two_primitives() {
        let (button, _ctx) = make_button_with_rect();
        let view = button.view();
        if let View::Group(children) = view {
            assert_eq!(children.len(), 2);
            assert!(matches!(
                &children[0],
                View::Primitive(DrawCommand::Rect { .. })
            ));
            assert!(matches!(
                &children[1],
                View::Primitive(DrawCommand::Text { .. })
            ));
        } else {
            panic!("expected Group");
        }
    }

    #[test]
    fn button_on_event_hover_changes_color() {
        let (mut button, _ctx) = make_button_with_rect();

        // No hover initially
        let view_normal = button.view();
        let color_normal = rect_fill_color(&view_normal);

        // Move inside rect
        button.on_event(&Event::PointerMoved {
            x: 1.0,
            y: 1.0,
            source: PointerSource::Mouse,
        });
        let view_hovered = button.view();
        let color_hovered = rect_fill_color(&view_hovered);

        assert_ne!(color_normal, color_hovered);

        // Move outside rect
        button.on_event(&Event::PointerMoved {
            x: 9999.0,
            y: 9999.0,
            source: PointerSource::Mouse,
        });
        let color_after = rect_fill_color(&button.view());
        assert_eq!(color_normal, color_after);
    }

    #[test]
    fn button_on_event_click_calls_callback() {
        let mut ctx = WidgetCtx::new();
        let flag = Rc::new(Cell::new(false));
        let flag_clone = flag.clone();
        let button = Button::new("OK", &mut ctx).on_click(move || flag_clone.set(true));
        let root = ctx.new_container(
            layout_core::LayoutStyle::new()
                .flex_column()
                .width(200.0)
                .height(100.0),
            &[button.layout_node()],
        );
        ctx.compute(root, 200.0, 100.0).unwrap();
        let mut button = button;

        let result = button.on_event(&Event::PointerPressed {
            x: 1.0,
            y: 1.0,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        });

        assert!(flag.get());
        assert!(matches!(result, EventResult::Handled));
    }

    #[test]
    fn button_on_event_click_outside_does_nothing() {
        let flag = Rc::new(Cell::new(false));
        let flag_clone = flag.clone();
        let mut ctx = WidgetCtx::new();
        let button = Button::new("OK", &mut ctx).on_click(move || flag_clone.set(true));
        let root = ctx.new_container(
            layout_core::LayoutStyle::new()
                .flex_column()
                .width(200.0)
                .height(100.0),
            &[button.layout_node()],
        );
        ctx.compute(root, 200.0, 100.0).unwrap();
        let mut button = button;

        let result = button.on_event(&Event::PointerPressed {
            x: 9999.0,
            y: 9999.0,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        });

        assert!(!flag.get());
        assert!(matches!(result, EventResult::Ignored));
    }

    fn rect_fill_color(view: &View) -> Color {
        if let View::Group(children) = view {
            if let View::Primitive(DrawCommand::Rect { style, .. }) = &children[0] {
                if let Some(fill) = style.fill {
                    return fill.color();
                }
            }
        }
        panic!("unexpected view shape");
    }
}
