use std::cell::Cell;
use std::rc::Rc;

use layout_core::{LayoutError, LayoutStyle, NodeId};
use platform_core::{Event, PointerButton};
use renderer_core::{
    BorderRadius, Color, DrawCommand, RectPayload, RectStyle, TextPayload, TextStyle,
};
use ui_tree::{Component, EventResult, RenderNode};

use crate::layout_item::LayoutItem;
use crate::layout_leaf::LayoutLeaf;

pub struct ButtonStyle {
    pub bg: Color,
    pub bg_hover: Color,
    pub fg: Color,
    pub radius: f32,
}

impl Default for ButtonStyle {
    fn default() -> Self {
        Self {
            bg: Color::rgba(0.24, 0.47, 0.98, 1.0),
            bg_hover: Color::rgba(0.15, 0.39, 0.92, 1.0),
            fg: Color::WHITE,
            radius: 4.0,
        }
    }
}

pub struct Button {
    label: Rc<str>,
    leaf: LayoutLeaf,
    on_click: Option<Box<dyn Fn()>>,
    style_fn: Box<dyn Fn() -> ButtonStyle>,
    // Cell<bool> here serves as state storage, not a redraw trigger. Redraws are driven by the event loop (on_event → request_redraw), not by signal writes. This is intentional: view() is called imperatively each frame, not reactively.
    hovered: Cell<bool>,
}

impl Button {
    pub fn new(
        ctx: &mut crate::context::WidgetCtx,
        label: impl Into<String>,
    ) -> Result<Self, LayoutError> {
        let leaf = LayoutLeaf::register(ctx, LayoutStyle::new().height(36.0).min_width(80.0))?;
        Ok(Self {
            label: Rc::from(label.into()),
            leaf,
            on_click: None,
            style_fn: Box::new(ButtonStyle::default),
            hovered: Cell::new(false),
        })
    }

    pub fn on_click(mut self, f: impl Fn() + 'static) -> Self {
        self.on_click = Some(Box::new(f));
        self
    }

    pub fn style(mut self, f: impl Fn() -> ButtonStyle + 'static) -> Self {
        self.style_fn = Box::new(f);
        self
    }
}

impl Component for Button {
    fn view(&self) -> RenderNode {
        let style = (self.style_fn)();
        let r = self.leaf.rect.get();
        let bg_color = if self.hovered.get() {
            style.bg_hover
        } else {
            style.bg
        };
        let text_color = style.fg;
        let border_radius = style.radius;
        let local = geometry_core::Rect {
            x: 0.0,
            y: 0.0,
            width: r.width,
            height: r.height,
        };

        self.leaf.positioned_view(RenderNode::group([
            RenderNode::Primitive(DrawCommand::Rect(Box::new(RectPayload {
                rect: local,
                style: RectStyle::default()
                    .with_fill(bg_color)
                    .with_radius(BorderRadius::all(border_radius)),
            }))),
            RenderNode::Primitive(DrawCommand::Text(Box::new(TextPayload {
                text: Rc::clone(&self.label),
                rect: local,
                style: TextStyle::new(14.0, text_color),
            }))),
        ]))
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

impl LayoutItem for Button {
    fn layout_node(&self) -> NodeId {
        self.leaf.node
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use layout_core::AvailableSpace;
    use platform_core::{Event, PointerButton, PointerSource};
    use renderer_core::{Color, DrawCommand, Paint};

    use super::*;
    use crate::context::{WidgetCtx, compute_layout, new_container};
    use crate::layout_item::LayoutItem;

    fn make_button_with_rect() -> Button {
        let mut ctx = WidgetCtx::new();
        let button = Button::new(&mut ctx, "OK").unwrap();
        let root = new_container(
            &mut ctx,
            layout_core::LayoutStyle::new()
                .flex_column()
                .width(200.0)
                .height(100.0),
            &[button.layout_node()],
        )
        .unwrap();
        compute_layout(
            &mut ctx,
            root,
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();
        button
    }

    #[test]
    fn button_view_renders_two_primitives() {
        let button = make_button_with_rect();
        let view = button.view();
        if let RenderNode::Transform { children, .. } = view {
            assert_eq!(children.len(), 1);
            if let RenderNode::Group(inner) = &children[0] {
                assert_eq!(inner.len(), 2);
                assert!(matches!(
                    &inner[0],
                    RenderNode::Primitive(DrawCommand::Rect(_))
                ));
                assert!(matches!(
                    &inner[1],
                    RenderNode::Primitive(DrawCommand::Text(_))
                ));
            } else {
                panic!("expected Group inside Translate");
            }
        } else {
            panic!("expected Translate");
        }
    }

    #[test]
    fn button_on_event_hover_changes_color() {
        let mut button = make_button_with_rect();

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
        let mut ctx = WidgetCtx::new();
        let mut button = Button::new(&mut ctx, "OK")
            .unwrap()
            .on_click(move || flag_clone.set(true));
        let root = new_container(
            &mut ctx,
            layout_core::LayoutStyle::new()
                .flex_column()
                .width(200.0)
                .height(100.0),
            &[button.layout_node()],
        )
        .unwrap();
        compute_layout(
            &mut ctx,
            root,
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();

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
        let mut button = Button::new(&mut ctx, "OK")
            .unwrap()
            .on_click(move || flag_clone.set(true));
        let root = new_container(
            &mut ctx,
            layout_core::LayoutStyle::new()
                .flex_column()
                .width(200.0)
                .height(100.0),
            &[button.layout_node()],
        )
        .unwrap();
        compute_layout(
            &mut ctx,
            root,
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();

        let result = button.on_event(&Event::PointerPressed {
            x: 9999.0,
            y: 9999.0,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        });

        assert!(!flag.get());
        assert!(matches!(result, EventResult::Ignored));
    }

    fn rect_fill_color(view: &RenderNode) -> Color {
        if let RenderNode::Transform { children, .. } = view {
            if let RenderNode::Group(inner) = &children[0] {
                if let RenderNode::Primitive(DrawCommand::Rect(p)) = &inner[0] {
                    if let Some(fill) = p.style.fill {
                        if let Paint::Solid(color) = fill {
                            return color;
                        }
                    }
                }
            }
        }
        panic!("unexpected view shape");
    }
}
