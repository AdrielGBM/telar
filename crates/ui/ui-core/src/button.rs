use std::cell::Cell;
use std::rc::Rc;

use layout_core::{LayoutError, LayoutStyle, NodeId};
use platform_core::{Event, PointerButton};
use renderer_core::{BorderRadius, Color, DrawCommand, RectStyle, TextStyle};
use ui_tree::{Component, EventResult, View};

use crate::layout_leaf::LayoutLeaf;

pub struct Button {
    label: Rc<str>,
    leaf: LayoutLeaf,
    bg: Color,
    hover_bg: Color,
    text_style: TextStyle,
    on_click: Option<Box<dyn Fn()>>,
    // Cell<bool> here serves as state storage, not a redraw trigger. Redraws are driven by the event loop (on_event → request_redraw), not by signal writes. This is intentional: view() is called imperatively each frame, not reactively.
    hovered: Cell<bool>,
}

impl Button {
    pub fn new(label: impl Into<String>) -> Result<Self, LayoutError> {
        let leaf = LayoutLeaf::register(LayoutStyle::new().height(36.0))?;
        Ok(Self {
            label: Rc::from(label.into()),
            leaf,
            bg: Color::from_rgb_u8(59, 130, 246),
            hover_bg: Color::from_rgb_u8(37, 99, 235),
            text_style: TextStyle::new(14.0, Color::WHITE),
            on_click: None,
            hovered: Cell::new(false),
        })
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
        self.leaf.node
    }
}

impl Component for Button {
    fn view(&self) -> View {
        let rect = self.leaf.rect.get();
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

    // NOTE: expects coords pre-adjusted to layout space; callers are responsible for subtracting any PushTransform offsets. DPI normalization (physical → logical pixels) is handled upstream by platform-winit before events are emitted.
    fn on_event(&mut self, event: &Event) -> EventResult {
        let rect = self.leaf.rect.get();
        match event {
            Event::PointerMoved { x, y, .. } => {
                let now = rect.contains(*x as f32, *y as f32);
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
                if rect.contains(*x as f32, *y as f32) {
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

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use layout_core::AvailableSpace;
    use platform_core::{Event, PointerButton, PointerSource};
    use renderer_core::{Color, DrawCommand, FillStyle};

    use super::*;
    use crate::context::{WidgetCtx, compute_layout, new_container, with_context};

    fn make_button_with_rect() -> (Button, WidgetCtx) {
        let (result, ctx) = with_context(WidgetCtx::new(), || {
            let button = Button::new("OK").unwrap();
            let root = new_container(
                layout_core::LayoutStyle::new()
                    .flex_column()
                    .width(200.0)
                    .height(100.0),
                &[button.layout_node()],
            )
            .unwrap();
            compute_layout(
                root,
                AvailableSpace::Definite(200.0),
                AvailableSpace::Definite(100.0),
            )
            .unwrap();
            button
        });
        (result, ctx)
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
        let flag = Rc::new(Cell::new(false));
        let flag_clone = flag.clone();
        let (mut button, _ctx) = with_context(WidgetCtx::new(), || {
            let button = Button::new("OK")
                .unwrap()
                .on_click(move || flag_clone.set(true));
            let root = new_container(
                layout_core::LayoutStyle::new()
                    .flex_column()
                    .width(200.0)
                    .height(100.0),
                &[button.layout_node()],
            )
            .unwrap();
            compute_layout(
                root,
                AvailableSpace::Definite(200.0),
                AvailableSpace::Definite(100.0),
            )
            .unwrap();
            button
        });

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
        let (mut button, _ctx) = with_context(WidgetCtx::new(), || {
            let button = Button::new("OK")
                .unwrap()
                .on_click(move || flag_clone.set(true));
            let root = new_container(
                layout_core::LayoutStyle::new()
                    .flex_column()
                    .width(200.0)
                    .height(100.0),
                &[button.layout_node()],
            )
            .unwrap();
            compute_layout(
                root,
                AvailableSpace::Definite(200.0),
                AvailableSpace::Definite(100.0),
            )
            .unwrap();
            button
        });

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
                    if let FillStyle::Solid(color) = fill {
                        return color;
                    }
                }
            }
        }
        panic!("unexpected view shape");
    }
}
