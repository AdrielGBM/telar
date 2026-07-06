use geometry_core::{Rect, Transform};
use layout_core::{LayoutError, LayoutStyle, NodeId};
use platform_core::{Event, Key, NamedKey, PointerButton, PointerSource};
use reactive_core::{Effect, RwSignal, effect, signal};
use renderer_core::RectStyle;
use ui_tree::{Component, EventResult, RenderNode};

use crate::context::WidgetCtx;
use crate::drag::DragGesture;
use crate::focus::{self, FocusId};
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
    // Resolved per `view()` (like `opacity`) so a `$signal`-driven transform re-reads its current value. Takes the laid-out `Rect` so rotate/scale can pivot on the box centre; `None` means identity (no wrapping node).
    transform: Box<dyn Fn(Rect) -> Option<[f32; 6]>>,
    children: TrackedChildren,
    // Optional tap gesture so a styled box can itself be pressable (a clickable card); children still hit-test first.
    press: PressGesture,
    // Optional drag gesture (slider/reorder/resize): reports the pointer position on press and each move.
    drag: DragGesture,
    // Fires with `true`/`false` as the mouse enters/leaves the box (mouse only, like the hover style).
    on_hover: Option<Box<dyn Fn(bool)>>,
    // Fires on every key press. Key events carry no pointer position, so they are broadcast to every widget
    // — this is a GLOBAL shortcut handler (there is no per-widget focus), not focused text input.
    on_key: Option<Box<dyn Fn(&Key)>>,
    // When set, the box is focusable: it joins the tab order, takes focus on tap, and handles Tab while
    // focused. `on_focus` observes the transitions.
    focus_id: Option<FocusId>,
    // Watches focus transitions for `on_focus`; dropping it (with the box) tears the subscription down.
    _focus_effect: Option<Effect>,
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
            transform: Box::new(|_| None),
            children,
            press: PressGesture::default(),
            drag: DragGesture::default(),
            on_hover: None,
            on_key: None,
            focus_id: None,
            _focus_effect: None,
        })
    }

    pub fn with_opacity(mut self, opacity: impl Fn() -> f32 + 'static) -> Self {
        self.opacity = Box::new(opacity);
        self
    }

    /// Apply an affine transform (rotate/scale/translate) to the whole box each `view()`. The closure
    /// takes the laid-out rect and returns the 2×3 matrix, or `None` for identity.
    pub fn with_transform(
        mut self,
        transform: impl Fn(Rect) -> Option<[f32; 6]> + 'static,
    ) -> Self {
        self.transform = Box::new(transform);
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

    /// Make the box draggable. The callback fires with the pointer position (layout space) on a press
    /// inside the box and on every move until release — even after the pointer leaves the box. Map the
    /// coordinate to a value (slider) or an offset (reorder/resize).
    pub fn on_drag(mut self, f: impl Fn(f32, f32) + 'static) -> Self {
        self.drag.set(f);
        self
    }

    /// Fire `f(true)` when the mouse enters the box and `f(false)` when it leaves (mouse only). Independent
    /// of `on_hover_style`: a box can observe hover without swapping its paint.
    pub fn on_hover(mut self, f: impl Fn(bool) + 'static) -> Self {
        self.on_hover = Some(Box::new(f));
        self
    }

    /// Fire `f(&key)` on every key press. This is a GLOBAL handler (key events reach every widget; there is
    /// no per-widget focus), so it suits app-level shortcuts, not focused text entry.
    pub fn on_key(mut self, f: impl Fn(&Key) + 'static) -> Self {
        self.on_key = Some(Box::new(f));
        self
    }

    /// Make the box focusable and fire `f(true)`/`f(false)` when it gains/loses keyboard focus. It joins
    /// the tab order (Tab/Shift-Tab reach it) and takes focus on tap. Use it to drive a focus ring or to
    /// build a custom focusable widget on top of a `box`.
    pub fn on_focus(mut self, f: impl Fn(bool) + 'static) -> Self {
        let id = *self.focus_id.get_or_insert_with(focus::next_id);
        focus::register(id);
        // An effect fires the callback only on an actual transition (its first run seeds `last`, no fire).
        let last = std::rc::Rc::new(std::cell::Cell::new(focus::is_focused(id)));
        self._focus_effect = Some(effect(move || {
            let now = focus::is_focused(id);
            if now != last.get() {
                last.set(now);
                f(now);
            }
        }));
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
        let composed = if opacity < 1.0 {
            RenderNode::layer(opacity, 0.0, [content])
        } else {
            content
        };
        match (self.transform)(r) {
            Some(matrix) => RenderNode::transform_with(matrix, [composed]),
            None => composed,
        }
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        // No tap/drag handler, hover style, or event callbacks: behave exactly as a plain container (pure routing).
        if !self.press.is_set()
            && !self.drag.is_set()
            && self.hover_style.is_none()
            && self.on_hover.is_none()
            && self.on_key.is_none()
            && self.focus_id.is_none()
        {
            return dispatch_container_event(&mut self.children, event);
        }
        let rect = self.rect.get();
        match event {
            // Moves are broadcast to all children (their hover) and also feed our own scroll-vs-tap and
            // hover tracking. Hover is mouse-only: touch has no "pointer left", so a tap would otherwise
            // leave the box stuck in its hover style.
            Event::PointerMoved { x, y, source } => {
                self.press.track_move(event);
                let dragged = self.drag.moved(event, rect) == EventResult::Handled;
                let child = dispatch_container_event(&mut self.children, event);
                let tracks_hover = self.hover_style.is_some() || self.on_hover.is_some();
                if tracks_hover && matches!(source, PointerSource::Mouse) {
                    let inside = rect.contains(*x as f32, *y as f32);
                    if inside != self.is_hovered.get() {
                        self.is_hovered.set(inside);
                        if let Some(cb) = &self.on_hover {
                            cb(inside);
                        }
                        return EventResult::Handled;
                    }
                }
                if dragged { EventResult::Handled } else { child }
            }
            // A child (e.g. an inner button) hit-tests first and wins; only a press on the bare box arms our tap/drag.
            Event::PointerPressed {
                x,
                y,
                button: PointerButton::Primary,
                ..
            } => {
                if dispatch_container_event(&mut self.children, event) == EventResult::Handled {
                    self.press.cancel();
                    self.drag.end();
                    return EventResult::Handled;
                }
                // A tap inside a focusable box takes focus (and consumes the press so focus sticks).
                let focused = match self.focus_id {
                    Some(id) if rect.contains(*x as f32, *y as f32) => {
                        focus::request(id);
                        true
                    }
                    _ => false,
                };
                let tapped =
                    self.press.is_set() && self.press.arm(event, rect) == EventResult::Handled;
                let dragged =
                    self.drag.is_set() && self.drag.press(event, rect) == EventResult::Handled;
                if tapped || dragged || focused {
                    EventResult::Handled
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
                    self.drag.end();
                    return EventResult::Handled;
                }
                let dragged = self.drag.end();
                let tapped =
                    self.press.is_set() && self.press.release(event, rect) == EventResult::Handled;
                if tapped || dragged {
                    EventResult::Handled
                } else {
                    EventResult::Ignored
                }
            }
            Event::CursorLeft => {
                self.press.cancel();
                self.drag.end();
                if (self.hover_style.is_some() || self.on_hover.is_some()) && self.is_hovered.get()
                {
                    self.is_hovered.set(false);
                    if let Some(cb) = &self.on_hover {
                        cb(false);
                    }
                }
                dispatch_container_event(&mut self.children, event)
            }
            // Broadcast (no pointer position): fire the global key handler, then keep routing to children.
            Event::KeyPressed { key, modifiers } => {
                // While this focusable box holds focus, Tab moves focus to the next/previous field.
                if let Some(id) = self.focus_id
                    && focus::is_focused(id)
                    && matches!(key, Key::Named(NamedKey::Tab))
                {
                    if modifiers.is_shift {
                        focus::focus_prev();
                    } else {
                        focus::focus_next();
                    }
                    return EventResult::Handled;
                }
                if let Some(cb) = &self.on_key {
                    cb(key);
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

impl Drop for StyledContainer {
    fn drop(&mut self) {
        // Drop the focus watcher first so releasing focus below doesn't fire `on_focus` during teardown.
        self._focus_effect.take();
        if let Some(id) = self.focus_id {
            focus::unregister(id);
        }
    }
}

/// Builds the affine matrix for a box's declarative `rotate`/`scale`/`translate` attributes, pivoting
/// rotation and scale on the box centre. Returns `None` when every component is identity, so an untransformed
/// box skips the extra transform node entirely.
pub fn box_transform(
    rect: Rect,
    rotate_deg: f32,
    scale_x: f32,
    scale_y: f32,
    translate_x: f32,
    translate_y: f32,
) -> Option<[f32; 6]> {
    if rotate_deg == 0.0
        && scale_x == 1.0
        && scale_y == 1.0
        && translate_x == 0.0
        && translate_y == 0.0
    {
        return None;
    }
    let cx = rect.x + rect.width / 2.0;
    let cy = rect.y + rect.height / 2.0;
    let matrix = Transform::rotate_around(rotate_deg, cx, cy)
        .then(Transform::scale_around(scale_x, scale_y, cx, cy))
        .then(Transform::translate(translate_x, translate_y));
    Some(matrix.to_array())
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

    #[test]
    fn box_transform_identity_is_none() {
        let r = Rect {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
        };
        assert!(box_transform(r, 0.0, 1.0, 1.0, 0.0, 0.0).is_none());
    }

    #[test]
    fn box_transform_scale_pivots_on_center() {
        let r = Rect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        };
        // scale_around(2, 2, 50, 50): pins the centre, so e = f = 50 - 2*50 = -50.
        assert_eq!(
            box_transform(r, 0.0, 2.0, 2.0, 0.0, 0.0).unwrap(),
            [2.0, 0.0, 0.0, 2.0, -50.0, -50.0]
        );
    }

    #[test]
    fn box_transform_translate_offsets_origin() {
        let r = Rect {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
        };
        assert_eq!(
            box_transform(r, 0.0, 1.0, 1.0, 8.0, -4.0).unwrap(),
            [1.0, 0.0, 0.0, 1.0, 8.0, -4.0]
        );
    }
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

    #[test]
    fn on_hover_fires_on_enter_and_leave() {
        let seen: Rc<Cell<Option<bool>>> = Rc::new(Cell::new(None));
        let sink = seen.clone();
        let mut ctx = WidgetCtx::new();
        let inner = Container::new(
            &mut ctx,
            LayoutStyle::new().width(100.0).height(100.0),
            vec![],
        )
        .unwrap();
        let mut card = StyledContainer::new(
            &mut ctx,
            LayoutStyle::new().flex_column().width(100.0).height(100.0),
            |_r| RectStyle::default(),
            vec![Box::new(inner)],
        )
        .unwrap()
        .on_hover(move |h| sink.set(Some(h)));
        let node = card.layout_node();
        compute_layout(
            &mut ctx,
            node,
            AvailableSpace::Definite(100.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();

        card.on_event(&Event::PointerMoved {
            x: 50.0,
            y: 50.0,
            source: PointerSource::Mouse,
        });
        assert_eq!(seen.get(), Some(true), "entering fires on_hover(true)");
        card.on_event(&Event::CursorLeft);
        assert_eq!(seen.get(), Some(false), "leaving fires on_hover(false)");
    }

    #[test]
    fn on_key_fires_on_key_press() {
        let count = Rc::new(Cell::new(0u32));
        let sink = count.clone();
        let mut ctx = WidgetCtx::new();
        let inner = Container::new(
            &mut ctx,
            LayoutStyle::new().width(10.0).height(10.0),
            vec![],
        )
        .unwrap();
        let mut card = StyledContainer::new(
            &mut ctx,
            LayoutStyle::new().flex_column(),
            |_r| RectStyle::default(),
            vec![Box::new(inner)],
        )
        .unwrap()
        .on_key(move |_k| sink.set(sink.get() + 1));
        card.on_event(&Event::KeyPressed {
            key: Key::Char('a'),
            modifiers: platform_core::ModifiersState::default(),
        });
        assert_eq!(count.get(), 1, "a key press fires on_key");
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

    // on_drag fires on a press inside, on every subsequent move (even once the pointer leaves the box),
    // then stops after release.
    #[test]
    fn on_drag_reports_press_then_moves_until_release() {
        use std::cell::RefCell;
        let seen: Rc<RefCell<Vec<(f32, f32)>>> = Rc::new(RefCell::new(Vec::new()));
        let sink = seen.clone();
        let mut ctx = WidgetCtx::new();
        let mut card = StyledContainer::new(
            &mut ctx,
            LayoutStyle::new().flex_column().width(200.0).height(200.0),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .on_drag(move |x, y| sink.borrow_mut().push((x, y)));
        compute_layout(
            &mut ctx,
            card.layout_node(),
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(200.0),
        )
        .unwrap();

        let moved = |x: f64, y: f64| Event::PointerMoved {
            x,
            y,
            source: PointerSource::Mouse,
        };
        card.on_event(&press(40.0, 40.0, PointerSource::Mouse));
        card.on_event(&moved(80.0, 90.0));
        card.on_event(&moved(400.0, 400.0)); // outside the box: drag still tracks
        card.on_event(&release(400.0, 400.0, PointerSource::Mouse));
        card.on_event(&moved(10.0, 10.0)); // after release: no longer dragging

        assert_eq!(
            *seen.borrow(),
            vec![(40.0, 40.0), (80.0, 90.0), (400.0, 400.0)],
            "drag reports the press point then each move until release"
        );
    }

    // Regression: a drag released OUTSIDE the widget must still end. Dispatched through a parent (whose
    // release path position-filters presses) — the release must broadcast to the dragging child anyway,
    // else it stays stuck to the pointer (fires on_drag on later moves).
    #[test]
    fn drag_released_outside_bounds_ends_via_parent_dispatch() {
        use std::cell::RefCell;
        let seen: Rc<RefCell<Vec<(f32, f32)>>> = Rc::new(RefCell::new(Vec::new()));
        let sink = seen.clone();
        let mut ctx = WidgetCtx::new();
        let child = StyledContainer::new(
            &mut ctx,
            LayoutStyle::new().width(100.0).height(100.0),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .on_drag(move |x, y| sink.borrow_mut().push((x, y)));
        let mut parent = Container::new(
            &mut ctx,
            LayoutStyle::new().flex_column().width(300.0).height(300.0),
            vec![Box::new(child)],
        )
        .unwrap();
        compute_layout(
            &mut ctx,
            parent.layout_node(),
            AvailableSpace::Definite(300.0),
            AvailableSpace::Definite(300.0),
        )
        .unwrap();

        let moved = |x: f64, y: f64| Event::PointerMoved {
            x,
            y,
            source: PointerSource::Mouse,
        };
        // child sits at (0,0) 100×100. Press inside, drag well outside, release outside.
        parent.on_event(&press(50.0, 50.0, PointerSource::Mouse));
        parent.on_event(&moved(250.0, 250.0));
        parent.on_event(&release(250.0, 250.0, PointerSource::Mouse));
        // After release the drag must be over: a later move fires nothing.
        parent.on_event(&moved(60.0, 60.0));
        assert_eq!(
            *seen.borrow(),
            vec![(50.0, 50.0), (250.0, 250.0)],
            "drag ended on the outside release; the post-release move must not fire"
        );
    }

    // A focusable box fires on_focus(true) when tapped and on_focus(false) when focus is cleared.
    #[test]
    fn on_focus_fires_on_gain_and_loss() {
        use std::cell::RefCell;
        let seen: Rc<RefCell<Vec<bool>>> = Rc::new(RefCell::new(Vec::new()));
        let sink = seen.clone();
        let mut ctx = WidgetCtx::new();
        let mut card = StyledContainer::new(
            &mut ctx,
            LayoutStyle::new().flex_column().width(100.0).height(100.0),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .on_focus(move |f| sink.borrow_mut().push(f));
        compute_layout(
            &mut ctx,
            card.layout_node(),
            AvailableSpace::Definite(100.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();

        card.on_event(&press(50.0, 50.0, PointerSource::Mouse)); // tap focuses → on_focus(true)
        crate::focus::clear(); // → on_focus(false)
        assert_eq!(
            *seen.borrow(),
            vec![true, false],
            "on_focus fires true on gain then false on loss"
        );
    }
}
