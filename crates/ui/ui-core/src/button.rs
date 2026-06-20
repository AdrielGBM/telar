use std::sync::Arc;

use layout_core::{LayoutError, LayoutStyle};
use platform_core::{Event, PointerButton};
use reactive_core::{RwSignal, create_rw_signal};
use renderer_core::{BorderRadius, Color, RectStyle, ShapeStyle, TextStyle};
use theme_core::use_widget_theme;
use ui_tree::{Component, EventResult, RenderNode};

use crate::impl_leaf_widget;
use crate::layout_leaf::LayoutLeaf;

fn darken(c: Color, factor: f32) -> Color {
    Color::rgba(
        (c.r * factor).min(1.0),
        (c.g * factor).min(1.0),
        (c.b * factor).min(1.0),
        c.a,
    )
}

pub struct ButtonStyle {
    pub rect: RectStyle,
    pub rect_hover: RectStyle,
    pub text: TextStyle,
    pub text_hover: TextStyle,
}

pub struct Button {
    label: Arc<str>,
    leaf: LayoutLeaf,
    on_click: Option<Box<dyn Fn()>>,
    style_fn: Box<dyn Fn() -> ButtonStyle>,
    hovered: RwSignal<bool>,
}

impl Button {
    pub fn new(
        ctx: &mut crate::context::WidgetCtx,
        label: impl Into<String>,
    ) -> Result<Self, LayoutError> {
        let leaf = LayoutLeaf::register(ctx, LayoutStyle::new().height(36.0).min_width(80.0))?;
        Ok(Self {
            label: Arc::from(label.into()),
            leaf,
            on_click: None,
            style_fn: Box::new(|| {
                let primary = use_widget_theme()
                    .map(|t| t.widget_primary())
                    .unwrap_or(Color::rgba(0.24, 0.47, 0.98, 1.0));
                let on_primary = use_widget_theme()
                    .map(|t| t.widget_on_primary())
                    .unwrap_or(Color::WHITE);
                ButtonStyle {
                    rect: RectStyle::default()
                        .with_fill(primary)
                        .with_radius(BorderRadius::all(4.0)),
                    rect_hover: RectStyle::default()
                        .with_fill(darken(primary, 0.85))
                        .with_radius(BorderRadius::all(4.0)),
                    text: TextStyle::new(14.0, on_primary),
                    text_hover: TextStyle::new(14.0, on_primary),
                }
            }),
            hovered: create_rw_signal(false),
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
        let hovered = self.hovered.get();
        let rect_style = if hovered {
            style.rect_hover
        } else {
            style.rect
        };
        let text_style = if hovered {
            style.text_hover
        } else {
            style.text
        };
        let local = geometry_core::Rect {
            x: 0.0,
            y: 0.0,
            width: r.width,
            height: r.height,
        };

        self.leaf.at_layout_position(RenderNode::group([
            RenderNode::rect(local, rect_style),
            RenderNode::text(Arc::clone(&self.label), local, text_style),
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

impl_leaf_widget!(Button);

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
            if let RenderNode::Group {
                children: inner, ..
            } = &children[0]
            {
                assert_eq!(inner.len(), 2);
                assert!(matches!(
                    &inner[0],
                    RenderNode::Primitive(DrawCommand::Rect { .. })
                ));
                assert!(matches!(
                    &inner[1],
                    RenderNode::Primitive(DrawCommand::Text { .. })
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
            if let RenderNode::Group {
                children: inner, ..
            } = &children[0]
            {
                if let RenderNode::Primitive(DrawCommand::Rect { style, .. }) = &inner[0] {
                    let fill = style.fill;
                    if let Some(Paint::Solid(color)) = fill {
                        return color;
                    }
                }
            }
        }
        panic!("unexpected view shape");
    }
}
