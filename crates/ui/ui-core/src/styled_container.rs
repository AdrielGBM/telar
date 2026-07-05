use geometry_core::Rect;
use layout_core::{LayoutError, LayoutStyle, NodeId};
use platform_core::{Event, PointerButton, PointerSource};
use reactive_core::{RwSignal, signal};
use renderer_core::RectStyle;
use ui_tree::{Component, EventResult, RenderNode};

use crate::context::WidgetCtx;
use crate::layout_item::{LayoutItem, TrackedChildren, register_container};
use crate::pointer::dispatch_container_event;
use crate::press::PressGesture;

pub struct StyledContainer {
    node: NodeId,
    rect: RwSignal<Rect>,
    style: Box<dyn Fn(Rect) -> RectStyle>,
    // Swapped in while the pointer is over the box (mouse only), mirroring `Button`'s rect/rect_hover.
    hover_style: Option<Box<dyn Fn(Rect) -> RectStyle>>,
    is_hovered: RwSignal<bool>,
    // A closure (not a plain f32) so `view()` re-reads it every run: a reactive opacity or a `transition:opacity` animation resolves to its current value on each re-render.
    opacity: Box<dyn Fn() -> f32>,
    children: TrackedChildren,
    // Optional tap gesture so a styled box can itself be pressable (a clickable card); children still hit-test first.
    press: PressGesture,
}

impl StyledContainer {
    pub fn new(
        ctx: &mut WidgetCtx,
        layout_style: LayoutStyle,
        style: impl Fn(Rect) -> RectStyle + 'static,
        children: Vec<Box<dyn LayoutItem>>,
    ) -> Result<Self, LayoutError> {
        let (node, rect, children) = register_container(ctx, layout_style, children)?;
        Ok(Self {
            node,
            rect,
            style: Box::new(style),
            hover_style: None,
            is_hovered: signal(false),
            opacity: Box::new(|| 1.0),
            children,
            press: PressGesture::default(),
        })
    }

    pub fn with_opacity(mut self, opacity: impl Fn() -> f32 + 'static) -> Self {
        self.opacity = Box::new(opacity);
        self
    }

    /// Paint the box with `f` while the mouse hovers it (a declarative style swap, like `Button`).
    /// Hover is mouse-only; touch never sets it, so a tap leaves no stuck hover state.
    pub fn on_hover_style(mut self, f: impl Fn(Rect) -> RectStyle + 'static) -> Self {
        self.hover_style = Some(Box::new(f));
        self
    }

    /// Make the box itself pressable. The callback fires on a tap (release, not press) inside the box;
    /// a child widget that handles the press wins, and a scroll gesture started on the box does not fire it.
    pub fn on_press(mut self, f: impl Fn() + 'static) -> Self {
        self.press.set(f);
        self
    }
}

impl LayoutItem for StyledContainer {
    fn layout_node(&self) -> NodeId {
        self.node
    }
}

impl Component for StyledContainer {
    fn view(&self) -> RenderNode {
        let r = self.rect.get();
        // Only subscribe to `is_hovered` when a hover style exists, so a plain box's view() stays inert.
        let style = match &self.hover_style {
            Some(hover) if self.is_hovered.get() => hover,
            _ => &self.style,
        };
        let background = RenderNode::rect(
            Rect {
                x: r.x,
                y: r.y,
                width: r.width,
                height: r.height,
            },
            style(r),
        );
        let content = RenderNode::group(
            std::iter::once(background).chain(self.children.iter().map(|c| c.segment.boundary())),
        );
        let opacity = (self.opacity)();
        if opacity < 1.0 {
            RenderNode::layer(opacity, 0.0, [content])
        } else {
            content
        }
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        // No tap handler and no hover style: behave exactly as a plain container (pure child routing).
        if !self.press.is_set() && self.hover_style.is_none() {
            return dispatch_container_event(&mut self.children, event);
        }
        let rect = self.rect.get();
        match event {
            // Moves are broadcast to all children (their hover) and also feed our own scroll-vs-tap and
            // hover tracking. Hover is mouse-only: touch has no "pointer left", so a tap would otherwise
            // leave the box stuck in its hover style.
            Event::PointerMoved { x, y, source } => {
                self.press.track_move(event);
                let child = dispatch_container_event(&mut self.children, event);
                if self.hover_style.is_some() && matches!(source, PointerSource::Mouse) {
                    let inside = rect.contains(*x as f32, *y as f32);
                    if inside != self.is_hovered.get() {
                        self.is_hovered.set(inside);
                        return EventResult::Handled;
                    }
                }
                child
            }
            // A child (e.g. an inner button) hit-tests first and wins; only a press on the bare box arms our tap.
            Event::PointerPressed {
                button: PointerButton::Primary,
                ..
            } => {
                if dispatch_container_event(&mut self.children, event) == EventResult::Handled {
                    self.press.cancel();
                    return EventResult::Handled;
                }
                if self.press.is_set() {
                    self.press.arm(event, rect)
                } else {
                    EventResult::Ignored
                }
            }
            Event::PointerReleased {
                button: PointerButton::Primary,
                ..
            } => {
                if dispatch_container_event(&mut self.children, event) == EventResult::Handled {
                    self.press.cancel();
                    return EventResult::Handled;
                }
                if self.press.is_set() {
                    self.press.release(event, rect)
                } else {
                    EventResult::Ignored
                }
            }
            Event::CursorLeft => {
                self.press.cancel();
                if self.hover_style.is_some() && self.is_hovered.get() {
                    self.is_hovered.set(false);
                }
                dispatch_container_event(&mut self.children, event)
            }
            _ => dispatch_container_event(&mut self.children, event),
        }
    }

    fn debug_name(&self) -> &'static str {
        "StyledContainer"
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use layout_core::AvailableSpace;
    use platform_core::{PointerButton, PointerSource};
    use renderer_core::{Color, ShapeStyle};
    use theme_core::{Theme, WidgetTheme, set_theme_with_widgets, use_theme};

    use super::*;
    use crate::button::Button;
    use crate::container::Container;
    use crate::context::{compute_layout, track_layout};

    fn press(x: f64, y: f64, source: PointerSource) -> Event {
        Event::PointerPressed {
            x,
            y,
            button: PointerButton::Primary,
            source,
        }
    }
    fn release(x: f64, y: f64, source: PointerSource) -> Event {
        Event::PointerReleased {
            x,
            y,
            button: PointerButton::Primary,
            source,
        }
    }

    #[derive(Clone)]
    struct TestTheme(Color);
    impl Theme for TestTheme {
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }
    impl WidgetTheme for TestTheme {
        fn widget_primary(&self) -> Color {
            self.0
        }
        fn widget_on_primary(&self) -> Color {
            Color::WHITE
        }
    }

    // Clicking a theme button (which sets the global THEME) while a themed StyledContainer ancestor is on the dispatch stack must not re-enter that ancestor's render segment mid borrow_mut.
    #[test]
    fn theme_button_click_force_tick_no_panic() {
        set_theme_with_widgets(TestTheme(Color::RED));

        let mut ctx = WidgetCtx::new();
        let btn = Button::new(&mut ctx, "x").unwrap();
        let btn_node = btn.layout_node();
        let btn = btn.on_click(move || set_theme_with_widgets(TestTheme(Color::GREEN)));
        let inner = Container::new(
            &mut ctx,
            LayoutStyle::new().flex_column().width(200.0).height(100.0),
            vec![Box::new(btn)],
        )
        .unwrap();
        let card = StyledContainer::new(
            &mut ctx,
            LayoutStyle::new().flex_column().width(200.0).height(100.0),
            |_r| RectStyle::default().with_fill(use_theme::<TestTheme>().0),
            vec![Box::new(inner)],
        )
        .unwrap();
        let card_node = card.layout_node();
        compute_layout(
            &mut ctx,
            card_node,
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();
        let br = track_layout(&ctx, btn_node).unwrap().get();

        let mut tree = crate::ComponentList::new(card);
        let _ = tree.commands();

        reactive_core::begin_batch();
        let handled = tree.on_event(&Event::PointerPressed {
            x: (br.x + br.width / 2.0) as f64,
            y: (br.y + br.height / 2.0) as f64,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        });
        if handled == EventResult::Handled {
            tree.bump_force_ticks();
            reactive_core::end_batch();
            reactive_core::begin_batch();
        }
        let _ = tree.commands();
        reactive_core::end_batch();
    }

    // A pressable box fires its callback on release (a tap), never on press alone.
    #[test]
    fn on_press_fires_on_tap_not_press() {
        let flag = Rc::new(Cell::new(false));
        let f = flag.clone();
        let mut ctx = WidgetCtx::new();
        let mut card = StyledContainer::new(
            &mut ctx,
            LayoutStyle::new().flex_column().width(200.0).height(100.0),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .on_press(move || f.set(true));
        compute_layout(
            &mut ctx,
            card.layout_node(),
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();

        assert_eq!(
            card.on_event(&press(100.0, 50.0, PointerSource::Mouse)),
            EventResult::Handled
        );
        assert!(!flag.get(), "press alone must not fire on_press");
        assert_eq!(
            card.on_event(&release(100.0, 50.0, PointerSource::Mouse)),
            EventResult::Handled
        );
        assert!(flag.get(), "release inside the box fires on_press");
    }

    // A child that handles the press (an inner button) wins; the box's own on_press must stay silent.
    #[test]
    fn inner_button_press_wins_over_box() {
        let card_flag = Rc::new(Cell::new(false));
        let btn_flag = Rc::new(Cell::new(false));
        let cf = card_flag.clone();
        let bf = btn_flag.clone();
        let mut ctx = WidgetCtx::new();
        let btn = Button::new(&mut ctx, "x")
            .unwrap()
            .on_click(move || bf.set(true));
        let btn_node = btn.layout_node();
        let mut card = StyledContainer::new(
            &mut ctx,
            LayoutStyle::new().flex_column().width(200.0).height(100.0),
            |_r| RectStyle::default(),
            vec![Box::new(btn)],
        )
        .unwrap()
        .on_press(move || cf.set(true));
        compute_layout(
            &mut ctx,
            card.layout_node(),
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();

        let br = track_layout(&ctx, btn_node).unwrap().get();
        let (cx, cy) = (
            (br.x + br.width / 2.0) as f64,
            (br.y + br.height / 2.0) as f64,
        );
        card.on_event(&press(cx, cy, PointerSource::Mouse));
        card.on_event(&release(cx, cy, PointerSource::Mouse));
        assert!(btn_flag.get(), "the inner button should fire");
        assert!(
            !card_flag.get(),
            "the box on_press must not fire when a child handled the press"
        );
    }

    // A hover style swaps the box's fill while the mouse is over it (mouse only), and clears on leave.
    #[test]
    fn hover_style_swaps_on_mouse_move() {
        let mut ctx = WidgetCtx::new();
        let mut card = StyledContainer::new(
            &mut ctx,
            LayoutStyle::new().flex_column().width(200.0).height(100.0),
            |_r| RectStyle::default().with_fill(Color::rgba(0.1, 0.1, 0.1, 1.0)),
            vec![],
        )
        .unwrap()
        .on_hover_style(|_r| RectStyle::default().with_fill(Color::rgba(0.9, 0.9, 0.9, 1.0)));
        compute_layout(
            &mut ctx,
            card.layout_node(),
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();

        let normal = fill_color(&card.view());
        card.on_event(&Event::PointerMoved {
            x: 100.0,
            y: 50.0,
            source: PointerSource::Mouse,
        });
        let hovered = fill_color(&card.view());
        assert_ne!(normal, hovered, "hover should swap the fill");

        card.on_event(&Event::PointerMoved {
            x: 9999.0,
            y: 9999.0,
            source: PointerSource::Mouse,
        });
        assert_eq!(
            fill_color(&card.view()),
            normal,
            "leaving the box restores the base fill"
        );
    }

    // Touch never sets hover (no "pointer left" on touch), so a tap leaves no stuck hover style.
    #[test]
    fn touch_move_does_not_set_hover() {
        let mut ctx = WidgetCtx::new();
        let mut card = StyledContainer::new(
            &mut ctx,
            LayoutStyle::new().flex_column().width(200.0).height(100.0),
            |_r| RectStyle::default().with_fill(Color::rgba(0.1, 0.1, 0.1, 1.0)),
            vec![],
        )
        .unwrap()
        .on_hover_style(|_r| RectStyle::default().with_fill(Color::rgba(0.9, 0.9, 0.9, 1.0)));
        compute_layout(
            &mut ctx,
            card.layout_node(),
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();

        let normal = fill_color(&card.view());
        card.on_event(&Event::PointerMoved {
            x: 100.0,
            y: 50.0,
            source: PointerSource::Touch { id: 1 },
        });
        assert_eq!(
            fill_color(&card.view()),
            normal,
            "a touch move must not trigger hover"
        );
    }

    fn fill_color(view: &RenderNode) -> Color {
        let group = match view {
            RenderNode::Group { children, .. } => children,
            _ => panic!("expected Group"),
        };
        if let RenderNode::Primitive(renderer_core::DrawCommand::Rect { style, .. }) = &group[0] {
            if let Some(renderer_core::Paint::Solid(c)) = style.fill {
                return c;
            }
        }
        panic!("expected a solid-fill background rect");
    }

    // A scroll gesture that begins on the box (press then drag past the slop) must not press it.
    #[test]
    fn scroll_drag_does_not_press_box() {
        let flag = Rc::new(Cell::new(false));
        let f = flag.clone();
        let mut ctx = WidgetCtx::new();
        let mut card = StyledContainer::new(
            &mut ctx,
            LayoutStyle::new().flex_column().width(200.0).height(200.0),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .on_press(move || f.set(true));
        compute_layout(
            &mut ctx,
            card.layout_node(),
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(200.0),
        )
        .unwrap();

        let touch = PointerSource::Touch { id: 1 };
        card.on_event(&press(50.0, 20.0, touch.clone()));
        card.on_event(&Event::PointerMoved {
            x: 50.0,
            y: 120.0, // > TAP_SLOP away
            source: touch.clone(),
        });
        card.on_event(&release(50.0, 120.0, touch));
        assert!(!flag.get(), "a scroll drag over the box must not press it");
    }
}
