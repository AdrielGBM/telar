use std::sync::Arc;

use layout_core::{LayoutError, LayoutStyle, MeasureFn};
use platform_core::{Event, PointerButton, PointerSource};
use reactive_core::{RwSignal, signal};
use renderer_core::{BorderRadius, Color, RectStyle, ShapeStyle, TextStyle};
use theme_core::use_widget_theme;
use ui_tree::{Component, EventResult, RenderNode};

use crate::impl_leaf_widget;
use crate::layout_leaf::LayoutLeaf;

/// Horizontal / vertical padding a content-sized button reserves around its label.
const PAD_X: f32 = 14.0;
const PAD_Y: f32 = 8.0;
/// Font size a content-sized button measures its label at (matches the default `ButtonStyle` text).
const DEFAULT_FONT_SIZE: f32 = 14.0;
/// Max pointer travel (logical px) from the press point still counted as a tap rather than a scroll/drag.
const TAP_SLOP: f32 = 10.0;

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
    style: Box<dyn Fn() -> ButtonStyle>,
    is_hovered: RwSignal<bool>,
    // Touch/scroll disambiguation: the press point while a tap is pending. on_click fires on release, not
    // press, and this is cleared once the pointer travels past TAP_SLOP — so a scroll gesture that begins
    // on a button (touch-down then drag) never triggers it.
    press_origin: Option<(f32, f32)>,
}

impl Button {
    /// Default button: sizes to its label plus padding (`PAD_X` / `PAD_Y`), height from the line box.
    pub fn new(
        ctx: &mut crate::context::WidgetCtx,
        label: impl Into<String>,
    ) -> Result<Self, LayoutError> {
        let label: Arc<str> = Arc::from(label.into());
        let measure_label = Arc::clone(&label);
        // Buttons are single-line, so measure at an unbounded width; the box is text + padding.
        let measure: MeasureFn = Box::new(move |_max_width: f32| {
            let (text_w, _) = renderer_text::measure_text(&measure_label, 1.0e6, DEFAULT_FONT_SIZE);
            let line_h = DEFAULT_FONT_SIZE * renderer_text::LINE_HEIGHT_FACTOR;
            (text_w + 2.0 * PAD_X, line_h + 2.0 * PAD_Y)
        });
        let (node, rect) = crate::context::new_measured_leaf(ctx, LayoutStyle::new(), measure)?;
        Ok(Self::from_leaf(label, LayoutLeaf { node, rect }))
    }

    /// Button with a caller-supplied layout style, for fixed sizes the default doesn't cover (e.g. a square icon button).
    pub fn with_layout(
        ctx: &mut crate::context::WidgetCtx,
        label: impl Into<String>,
        layout_style: LayoutStyle,
    ) -> Result<Self, LayoutError> {
        let leaf = LayoutLeaf::register(ctx, layout_style)?;
        Ok(Self::from_leaf(Arc::from(label.into()), leaf))
    }

    fn from_leaf(label: Arc<str>, leaf: LayoutLeaf) -> Self {
        Self {
            label,
            leaf,
            on_click: None,
            style: Box::new(|| {
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
                        .with_fill(primary.darken(0.15))
                        .with_radius(BorderRadius::all(4.0)),
                    text: TextStyle::new(DEFAULT_FONT_SIZE, on_primary),
                    text_hover: TextStyle::new(DEFAULT_FONT_SIZE, on_primary),
                }
            }),
            is_hovered: signal(false),
            press_origin: None,
        }
    }

    pub fn on_click(mut self, f: impl Fn() + 'static) -> Self {
        self.on_click = Some(Box::new(f));
        self
    }

    pub fn style(mut self, f: impl Fn() -> ButtonStyle + 'static) -> Self {
        self.style = Box::new(f);
        self
    }
}

impl Component for Button {
    fn view(&self) -> RenderNode {
        let style = (self.style)();
        let r = self.leaf.rect.get();
        let is_hovered = self.is_hovered.get();
        let rect_style = if is_hovered {
            style.rect_hover
        } else {
            style.rect
        };
        let text_style = if is_hovered {
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

        // TextStyle carries no alignment, so center the label. The renderer top-aligns the baseline at
        // rect.y, so vertical centering is done via rect.y against the true single-line height — not
        // measure_text's height, which is padded for layout reservation (baseline + a full line box).
        let (text_w, _) = renderer_text::measure_text(&self.label, r.width, text_style.font_size);
        let line_h = text_style.font_size * renderer_text::LINE_HEIGHT_FACTOR;
        let text_rect = geometry_core::Rect {
            x: ((r.width - text_w) * 0.5).max(0.0),
            y: ((r.height - line_h) * 0.5).max(0.0),
            width: text_w,
            height: line_h,
        };

        self.leaf.at_layout_position(RenderNode::group([
            RenderNode::rect(local, rect_style),
            RenderNode::text(Arc::clone(&self.label), text_rect, text_style),
        ]))
    }

    // Pointer coords must already be in layout space; callers subtract any PushTransform offset.
    fn on_event(&mut self, event: &Event) -> EventResult {
        let rect = self.leaf.rect.get();
        match event {
            Event::PointerMoved { x, y, source } => {
                // Past the slop the gesture is a scroll/drag, not a tap: drop the pending press so the
                // release won't fire on_click. This is what stops a scroll begun on a button from clicking.
                if let Some((ox, oy)) = self.press_origin {
                    let (dx, dy) = (*x as f32 - ox, *y as f32 - oy);
                    if dx * dx + dy * dy > TAP_SLOP * TAP_SLOP {
                        self.press_origin = None;
                    }
                }
                // Hover is a mouse-only concept. Touch has no "pointer left" event, so tracking hover on a
                // touch move leaves the button stuck in its hover style after a tap.
                if matches!(source, PointerSource::Mouse) {
                    let is_inside = rect.contains(*x as f32, *y as f32);
                    if is_inside != self.is_hovered.get() {
                        self.is_hovered.set(is_inside);
                        return EventResult::Handled;
                    }
                }
                EventResult::Ignored
            }
            // Arm a candidate tap; on_click fires on release, not here, so a scroll gesture starting on the
            // button (touch-down then drag) doesn't trigger it.
            Event::PointerPressed {
                x,
                y,
                button: PointerButton::Primary,
                ..
            } => {
                if rect.contains(*x as f32, *y as f32) {
                    self.press_origin = Some((*x as f32, *y as f32));
                    EventResult::Handled
                } else {
                    self.press_origin = None;
                    EventResult::Ignored
                }
            }
            // A tap completes only if the press landed here and the release is still on the button (a drag
            // past the slop already cleared press_origin). Clear hover too (a touch tap can synthesize a move).
            Event::PointerReleased {
                x,
                y,
                button: PointerButton::Primary,
                ..
            } => {
                let armed = self.press_origin.take().is_some();
                if self.is_hovered.get() {
                    self.is_hovered.set(false);
                }
                if armed && rect.contains(*x as f32, *y as f32) {
                    if let Some(cb) = &self.on_click {
                        cb();
                    }
                    EventResult::Handled
                } else {
                    EventResult::Ignored
                }
            }
            // Any other release / cursor-leave cancels the pending tap and clears hover, then keeps propagating.
            Event::PointerReleased { .. } | Event::CursorLeft => {
                self.press_origin = None;
                if self.is_hovered.get() {
                    self.is_hovered.set(false);
                }
                EventResult::Ignored
            }
            _ => EventResult::Ignored,
        }
    }

    fn debug_name(&self) -> &'static str {
        "Button"
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
    fn button_sizes_to_content_plus_padding() {
        let mut ctx = WidgetCtx::new();
        let button = Button::new(&mut ctx, "OK").unwrap();
        let node = button.layout_node();
        let rect_sig = crate::context::track_layout(&ctx, node).unwrap();
        // A row with start alignment leaves both axes at the button's measured size (no stretch).
        let root = new_container(
            &mut ctx,
            layout_core::LayoutStyle::new()
                .flex_row()
                .align_items(layout_core::AlignItems::FLEX_START)
                .width(400.0)
                .height(100.0),
            &[node],
        )
        .unwrap();
        compute_layout(
            &mut ctx,
            root,
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();

        let rect = rect_sig.get();
        let (text_w, _) = renderer_text::measure_text("OK", 1.0e6, super::DEFAULT_FONT_SIZE);
        let line_h = super::DEFAULT_FONT_SIZE * renderer_text::LINE_HEIGHT_FACTOR;
        assert!(
            (rect.width - (text_w + 2.0 * super::PAD_X)).abs() < 0.5,
            "width not content+padding: got {} want {}",
            rect.width,
            text_w + 2.0 * super::PAD_X
        );
        assert!(
            (rect.height - (line_h + 2.0 * super::PAD_Y)).abs() < 0.5,
            "height not line+padding: got {} want {}",
            rect.height,
            line_h + 2.0 * super::PAD_Y
        );
    }

    #[test]
    fn measured_buttons_stack_in_column() {
        let mut ctx = WidgetCtx::new();
        let b0 = Button::new(&mut ctx, "One").unwrap();
        let b1 = Button::new(&mut ctx, "Two").unwrap();
        let b2 = Button::new(&mut ctx, "Three").unwrap();
        let r0 = crate::context::track_layout(&ctx, b0.layout_node()).unwrap();
        let r1 = crate::context::track_layout(&ctx, b1.layout_node()).unwrap();
        let r2 = crate::context::track_layout(&ctx, b2.layout_node()).unwrap();
        let col = new_container(
            &mut ctx,
            layout_core::LayoutStyle::new().flex_column().gap(3.0),
            &[b0.layout_node(), b1.layout_node(), b2.layout_node()],
        )
        .unwrap();
        compute_layout(
            &mut ctx,
            col,
            AvailableSpace::Definite(208.0),
            AvailableSpace::MaxContent,
        )
        .unwrap();
        assert!(
            r1.get().y > r0.get().y,
            "b1 should be below b0: {:?} {:?}",
            r0.get(),
            r1.get()
        );
        assert!(r2.get().y > r1.get().y, "b2 should be below b1");
    }

    #[test]
    fn button_label_is_centered() {
        let mut ctx = WidgetCtx::new();
        let button = Button::with_layout(
            &mut ctx,
            "OK",
            layout_core::LayoutStyle::new().width(40.0).height(40.0),
        )
        .unwrap();
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

        let RenderNode::Transform { children, .. } = button.view() else {
            panic!("expected Transform");
        };
        let RenderNode::Group {
            children: inner, ..
        } = &children[0]
        else {
            panic!("expected Group");
        };
        let RenderNode::Primitive(DrawCommand::Text { rect, .. }) = &inner[1] else {
            panic!("expected Text primitive");
        };
        // Vertical: the label's line box is centered in the 40px button (not pinned to the top).
        let line_h = 14.0 * renderer_text::LINE_HEIGHT_FACTOR;
        assert!(
            (rect.y - (40.0 - line_h) / 2.0).abs() < 0.5,
            "label not vertically centered: y={} line_h={line_h}",
            rect.y
        );
        // Horizontal: equal margins left and right.
        assert!(
            (rect.x - (40.0 - rect.width) / 2.0).abs() < 0.5,
            "label not horizontally centered: x={} w={}",
            rect.x,
            rect.width
        );
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

        let view_normal = button.view();
        let color_normal = rect_fill_color(&view_normal);

        button.on_event(&Event::PointerMoved {
            x: 1.0,
            y: 1.0,
            source: PointerSource::Mouse,
        });
        let view_hovered = button.view();
        let color_hovered = rect_fill_color(&view_hovered);

        assert_ne!(color_normal, color_hovered);

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

        // The tap fires on release, not press, so a scroll begun on the button doesn't click it.
        let pressed = button.on_event(&Event::PointerPressed {
            x: 1.0,
            y: 1.0,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        });
        assert!(matches!(pressed, EventResult::Handled));
        assert!(!flag.get(), "press alone must not fire the callback");

        let released = button.on_event(&Event::PointerReleased {
            x: 1.0,
            y: 1.0,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        });
        assert!(flag.get(), "release on the button fires the callback");
        assert!(matches!(released, EventResult::Handled));
    }

    #[test]
    fn button_scroll_gesture_does_not_click() {
        // Press on the button, then drag past the slop (a scroll): the release must NOT fire the callback.
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

        button.on_event(&Event::PointerPressed {
            x: 5.0,
            y: 5.0,
            button: PointerButton::Primary,
            source: PointerSource::Touch { id: 1 },
        });
        button.on_event(&Event::PointerMoved {
            x: 5.0,
            y: 40.0, // > TAP_SLOP away
            source: PointerSource::Touch { id: 1 },
        });
        button.on_event(&Event::PointerReleased {
            x: 5.0,
            y: 40.0,
            button: PointerButton::Primary,
            source: PointerSource::Touch { id: 1 },
        });
        assert!(
            !flag.get(),
            "a scroll drag over the button must not click it"
        );
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
