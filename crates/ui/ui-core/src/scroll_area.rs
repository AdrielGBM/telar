use std::cell::RefCell;
use std::rc::Rc;

use geometry_core::Rect;
use layout_core::{AvailableSpace, LayoutError, LayoutStyle};
use platform_core::{Event, ScrollDelta};
use reactive_core::{Effect, RwSignal, effect, signal};
use renderer_core::{BorderRadius, Color, RectStyle, ShapeStyle};
use theme_core::use_widget_theme;
use ui_tree::{Component, EventResult, RenderNode, Segment};

use ui_tree::NodeVec;

use crate::context::{WidgetCtx, track_layout};
use crate::impl_leaf_widget;
use crate::layout_item::{LayoutItem, mount_item_segment};
use crate::layout_leaf::LayoutLeaf;
use crate::pointer::{clip_pointer_event, offset_pointer};

pub struct ScrollbarStyle {
    pub color: Color,
    pub width: f32,
    pub corner_radius: f32,
}

impl Default for ScrollbarStyle {
    fn default() -> Self {
        let color = use_widget_theme()
            .map(|t| t.widget_scrollbar())
            .unwrap_or(Color::rgba(0.5, 0.5, 0.6, 0.6));
        Self {
            color,
            width: 8.0,
            corner_radius: 3.0,
        }
    }
}

fn draw_scrollbars(
    viewport: Rect,
    scroll_x: f32,
    scroll_y: f32,
    content_rect: Rect,
    scrollbar_style: &ScrollbarStyle,
) -> (RenderNode, RenderNode) {
    let vbar = if content_rect.height > viewport.height {
        let bar_h = (viewport.height / content_rect.height * viewport.height).max(24.0);
        let max_scroll = (content_rect.height - viewport.height).max(1.0);
        let bar_y = viewport.y + (scroll_y / max_scroll) * (viewport.height - bar_h);
        RenderNode::rect(
            Rect::new(
                viewport.x + viewport.width - scrollbar_style.width,
                bar_y,
                scrollbar_style.width - 2.0,
                bar_h,
            ),
            RectStyle::default()
                .with_fill(scrollbar_style.color)
                .with_radius(BorderRadius::all(scrollbar_style.corner_radius)),
        )
    } else {
        RenderNode::Empty
    };

    let hbar = if content_rect.width > viewport.width {
        let bar_w = (viewport.width / content_rect.width * viewport.width).max(24.0);
        let max_scroll_x = (content_rect.width - viewport.width).max(1.0);
        let bar_x = viewport.x + (scroll_x / max_scroll_x) * (viewport.width - bar_w);
        RenderNode::rect(
            Rect::new(
                bar_x,
                viewport.y + viewport.height - scrollbar_style.width,
                bar_w,
                scrollbar_style.width - 2.0,
            ),
            RectStyle::default()
                .with_fill(scrollbar_style.color)
                .with_radius(BorderRadius::all(scrollbar_style.corner_radius)),
        )
    } else {
        RenderNode::Empty
    };

    (vbar, hbar)
}

fn handle_scroll_event(
    event: &Event,
    viewport: Rect,
    scroll_x: RwSignal<f32>,
    scroll_y: RwSignal<f32>,
    content_rect_signal: RwSignal<Rect>,
    content: &Rc<RefCell<Box<dyn LayoutItem>>>,
    last_pointer: Option<(f32, f32)>,
) -> EventResult {
    if let Event::Scrolled { delta } = event {
        // Nested scroll: offer the wheel to the content first, so an inner scroll area under the pointer
        // consumes it before this (outer) one does.
        if content.borrow_mut().on_event(event) == EventResult::Handled {
            return EventResult::Handled;
        }
        // A wheel event carries no position, so only scroll here if the last pointer move was inside this
        // viewport; otherwise leave it Ignored for an ancestor scroll area to handle.
        if last_pointer.is_some_and(|(px, py)| !viewport.contains(px, py)) {
            return EventResult::Ignored;
        }
        let (delta_x, delta_y) = match delta {
            ScrollDelta::Lines { x, y } => (*x * 20.0, *y * 20.0),
            ScrollDelta::Pixels { x, y } => (*x, *y),
        };
        let content_rect = content_rect_signal.get();
        let max_scroll_x = (content_rect.width - viewport.width).max(0.0);
        let max_scroll_y = (content_rect.height - viewport.height).max(0.0);
        scroll_x.set((scroll_x.get() - delta_x).clamp(0.0, max_scroll_x));
        scroll_y.set((scroll_y.get() - delta_y).clamp(0.0, max_scroll_y));
        return EventResult::Handled;
    }

    let Some(event) = clip_pointer_event(event, viewport) else {
        return EventResult::Ignored;
    };

    let scroll_offset_x = scroll_x.get() as f64;
    let scroll_offset_y = scroll_y.get() as f64;
    let adjusted = offset_pointer(
        event,
        viewport.x as f64 - scroll_offset_x,
        viewport.y as f64 - scroll_offset_y,
    );
    let effective = adjusted.as_ref().unwrap_or(event);
    content.borrow_mut().on_event(effective)
}

pub(crate) struct ScrollCore {
    content_rect_signal: RwSignal<Rect>,
    scroll_x: RwSignal<f32>,
    scroll_y: RwSignal<f32>,
    // Shared between event dispatch (borrow_mut) and the content segment (borrow). The content is its own segment so a scroll tick only re-runs this core's view() to rewrite the Transform matrix — the content is referenced as a cheap boundary and is NOT re-flattened on scroll.
    content: Rc<RefCell<Box<dyn LayoutItem>>>,
    content_segment: Rc<Segment>,
    scrollbar_style: ScrollbarStyle,
    // Touch tap-vs-scroll: finger travel accumulated while a pointer is pressed. Content children see
    // content-space coords pinned under the finger during a drag, so they can't detect the scroll themselves;
    // once travel passes SCROLL_TAP_SLOP the scroll area cancels their pending tap (one CursorLeft) so a scroll
    // doesn't click. Gated on `press_active` so a mouse wheel scroll (no press) never cancels anything.
    press_active: bool,
    gesture_scroll: f32,
    tap_cancelled: bool,
    // Last pointer position seen (this area's own coordinate space). A wheel `Scrolled` event carries no
    // position, so nested scroll routing uses this to decide whether the pointer is over this viewport.
    last_pointer: Option<(f32, f32)>,
}

/// Accumulated finger travel (logical px) within a gesture past which the scroll area treats it as a scroll
/// and cancels any pending tap on its content.
const SCROLL_TAP_SLOP: f32 = 8.0;

impl ScrollCore {
    fn new(content_rect_signal: RwSignal<Rect>, content: Box<dyn LayoutItem>) -> Self {
        let content = Rc::new(RefCell::new(content));
        let content_segment = mount_item_segment(Rc::clone(&content));
        Self {
            content_rect_signal,
            scroll_x: signal(0.0),
            scroll_y: signal(0.0),
            content,
            content_segment,
            scrollbar_style: ScrollbarStyle::default(),
            press_active: false,
            gesture_scroll: 0.0,
            tap_cancelled: false,
            last_pointer: None,
        }
    }

    fn scroll_to_top(&mut self) {
        if self.scroll_x.get() != 0.0 {
            self.scroll_x.set(0.0);
        }
        if self.scroll_y.get() != 0.0 {
            self.scroll_y.set(0.0);
        }
    }

    fn clamp_scroll(&mut self, viewport: Rect) {
        let content_rect = self.content_rect_signal.get();
        let max_x = (content_rect.width - viewport.width).max(0.0);
        let max_y = (content_rect.height - viewport.height).max(0.0);
        let clamped_x = self.scroll_x.get().clamp(0.0, max_x);
        let clamped_y = self.scroll_y.get().clamp(0.0, max_y);
        if self.scroll_x.get() != clamped_x {
            self.scroll_x.set(clamped_x);
        }
        if self.scroll_y.get() != clamped_y {
            self.scroll_y.set(clamped_y);
        }
    }

    fn view(&self, viewport: Rect) -> RenderNode {
        let scroll_x = self.scroll_x.get();
        let scroll_y = self.scroll_y.get();
        let content_rect = self.content_rect_signal.get();
        let scrollable = RenderNode::Clip {
            rect: viewport,
            radius: BorderRadius::zero(),
            children: NodeVec::collect([RenderNode::Transform {
                matrix: [
                    1.0,
                    0.0,
                    0.0,
                    1.0,
                    viewport.x - scroll_x,
                    viewport.y - scroll_y,
                ],
                children: NodeVec::collect([self.content_segment.boundary()]),
            }]),
        };
        let (vbar, hbar) = draw_scrollbars(
            viewport,
            scroll_x,
            scroll_y,
            content_rect,
            &self.scrollbar_style,
        );
        RenderNode::group([scrollable, vbar, hbar])
    }

    fn on_event(&mut self, event: &Event, viewport: Rect) -> EventResult {
        match event {
            // A new press starts a fresh tap candidate; forget the prior gesture's accumulated scroll.
            Event::PointerPressed { .. } => {
                self.press_active = true;
                self.gesture_scroll = 0.0;
                self.tap_cancelled = false;
            }
            Event::PointerReleased { .. } => self.press_active = false,
            // Remember where the pointer is so a subsequent (position-less) wheel `Scrolled` can tell
            // whether it belongs to this viewport or an ancestor's.
            Event::PointerMoved { x, y, .. } => self.last_pointer = Some((*x as f32, *y as f32)),
            // While a pointer is down (a touch drag, not a mouse wheel), once the finger has travelled past
            // the slop this gesture is a scroll, not a tap: cancel the pending press on the content once (it
            // sees pinned content-space coords and can't tell on its own).
            Event::Scrolled { delta } if self.press_active && !self.tap_cancelled => {
                let (dx, dy) = match delta {
                    ScrollDelta::Lines { x, y } => (*x * 20.0, *y * 20.0),
                    ScrollDelta::Pixels { x, y } => (*x, *y),
                };
                self.gesture_scroll += (dx * dx + dy * dy).sqrt();
                if self.gesture_scroll > SCROLL_TAP_SLOP {
                    self.tap_cancelled = true;
                    self.content.borrow_mut().on_event(&Event::CursorLeft);
                }
            }
            _ => {}
        }
        handle_scroll_event(
            event,
            viewport,
            self.scroll_x.clone(),
            self.scroll_y.clone(),
            self.content_rect_signal.clone(),
            &self.content,
            self.last_pointer,
        )
    }
}

// Closure-viewport fixture exercising ScrollCore directly; test-only since LayoutScrollArea is the single public scroll-area.
#[cfg(test)]
struct ScrollArea {
    viewport: Box<dyn Fn() -> Rect>,
    core: ScrollCore,
}

#[cfg(test)]
impl ScrollArea {
    fn new(
        ctx: &WidgetCtx,
        viewport: impl Fn() -> Rect + 'static,
        content: Box<dyn LayoutItem>,
    ) -> Self {
        let content_rect_signal =
            track_layout(ctx, content.layout_node()).expect("content node not registered in ctx");
        Self {
            viewport: Box::new(viewport),
            core: ScrollCore::new(content_rect_signal, content),
        }
    }
}

#[cfg(test)]
impl Component for ScrollArea {
    fn view(&self) -> RenderNode {
        self.core.view((self.viewport)())
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        self.core.on_event(event, (self.viewport)())
    }
}

// Taffy-layout viewport; always valid as a LayoutItem — no panic possible.
pub struct LayoutScrollArea {
    leaf: LayoutLeaf,
    core: ScrollCore,
    // Lays the detached content subtree out against the viewport width whenever the viewport is (re)sized,
    // so a `scroll` element works on its own — its content is not a taffy child of the viewport leaf, so
    // nothing else would lay it out (the app shell computes only its OWN top-level scroll by hand).
    _layout_effect: Effect,
}

impl LayoutScrollArea {
    pub fn new(
        ctx: &mut WidgetCtx,
        layout_style: LayoutStyle,
        content: Box<dyn LayoutItem>,
    ) -> Result<Self, LayoutError> {
        let content_node = content.layout_node();
        let content_rect_signal =
            track_layout(ctx, content_node).expect("content node not registered in ctx");
        let leaf = LayoutLeaf::register(ctx, layout_style)?;

        // Re-lay out the content at the viewport width (unbounded height, so it can overflow and scroll)
        // each time the viewport resizes. The viewport rect is set by the surrounding layout; this effect
        // fires during that flush — after the runtime borrow is released — so computing here is re-entrancy
        // safe (same pattern as reactive lists).
        let viewport = leaf.rect.clone();
        let layout_effect = effect(move || {
            let vp = viewport.get();
            if vp.width > 0.0 {
                let _ = crate::context::compute_layout_root(
                    content_node,
                    AvailableSpace::Definite(vp.width),
                    AvailableSpace::MaxContent,
                );
            }
        });

        Ok(Self {
            leaf,
            core: ScrollCore::new(content_rect_signal, content),
            _layout_effect: layout_effect,
        })
    }

    pub fn scrollbar_style(mut self, style: ScrollbarStyle) -> Self {
        self.core.scrollbar_style = style;
        self
    }

    pub fn clamp_scroll(&mut self) {
        self.core.clamp_scroll(self.leaf.rect.get());
    }

    /// Resets the scroll offset to the top-left, e.g. when swapping the content shown in the viewport.
    pub fn scroll_to_top(&mut self) {
        self.core.scroll_to_top();
    }

    pub fn viewport_rect(&self) -> Rect {
        self.leaf.rect.get()
    }
}

impl_leaf_widget!(LayoutScrollArea);

impl Component for LayoutScrollArea {
    fn view(&self) -> RenderNode {
        self.core.view(self.leaf.rect.get())
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        self.core.on_event(event, self.leaf.rect.get())
    }

    fn debug_name(&self) -> &'static str {
        "ScrollArea"
    }
}

#[cfg(test)]
mod tests {
    use geometry_core::Rect;
    use layout_core::{AvailableSpace, LayoutStyle, NodeId};
    use platform_core::{Event, PointerSource, ScrollDelta};
    use renderer_core::DrawCommand;
    use ui_tree::{Component, EventResult, RenderNode};

    use super::*;
    use crate::canvas::Canvas;
    use crate::context::{WidgetCtx, compute_layout, new_container, track_layout};
    use crate::layout_item::LayoutItem;
    use crate::layout_leaf::LayoutLeaf;

    // Laying out only the scroll node must, via its effect, lay out the detached content subtree too —
    // a `scroll` element has no other owner to compute its content.
    #[test]
    fn scroll_area_lays_out_its_detached_content() {
        let mut ctx = WidgetCtx::new();
        let content = Canvas::new(
            &mut ctx,
            LayoutStyle::new().width(400.0).height(1000.0),
            |_| RenderNode::Empty,
        )
        .unwrap();
        let content_node = content.layout_node();
        let scroll = LayoutScrollArea::new(
            &mut ctx,
            LayoutStyle::new().width(300.0).height(160.0),
            Box::new(content),
        )
        .unwrap();
        let scroll_node = scroll.layout_node();
        compute_layout(
            &mut ctx,
            scroll_node,
            AvailableSpace::Definite(300.0),
            AvailableSpace::Definite(160.0),
        )
        .unwrap();
        let content_rect = track_layout(&ctx, content_node).unwrap().get();
        assert!(
            content_rect.height > 0.0,
            "scroll must lay out its content, got {content_rect:?}"
        );
    }

    fn make_scroll_area() -> ScrollArea {
        let mut ctx = WidgetCtx::new();
        let ctx = &mut ctx;
        let content = Canvas::new(ctx, LayoutStyle::new().width(400.0).height(1000.0), |_| {
            RenderNode::Empty
        })
        .unwrap();
        let node = content.layout_node();
        let sa = ScrollArea::new(ctx, || Rect::new(0.0, 0.0, 400.0, 300.0), Box::new(content));
        compute_layout(
            ctx,
            node,
            AvailableSpace::Definite(400.0),
            AvailableSpace::MaxContent,
        )
        .unwrap();
        sa
    }

    fn make_scroll_area_small() -> ScrollArea {
        let mut ctx = WidgetCtx::new();
        let ctx = &mut ctx;
        let content = Canvas::new(ctx, LayoutStyle::new().width(400.0).height(200.0), |_| {
            RenderNode::Empty
        })
        .unwrap();
        let node = content.layout_node();
        let sa = ScrollArea::new(ctx, || Rect::new(0.0, 0.0, 400.0, 300.0), Box::new(content));
        compute_layout(
            ctx,
            node,
            AvailableSpace::Definite(400.0),
            AvailableSpace::MaxContent,
        )
        .unwrap();
        sa
    }

    #[test]
    fn scroll_content_click_force_tick_no_panic() {
        use crate::button::Button;
        use crate::container::Container;
        use crate::context::track_layout;
        use platform_core::PointerButton;
        use reactive_core::{begin_batch, end_batch, signal};

        let mut ctx = WidgetCtx::new();
        let s = signal(0i32);
        let s_cb = s.clone();
        let btn = Button::new(&mut ctx, "x").unwrap();
        let btn_node = btn.layout_node();
        let btn = btn.on_click(move || s_cb.update(|n| *n += 1));
        let s_txt = s.clone();
        let txt = crate::text::Text::new(
            &mut ctx,
            move || format!("{}", s_txt.get()),
            LayoutStyle::new().width(50.0).height(20.0),
            || renderer_core::TextStyle::new(14.0, renderer_core::Color::BLACK),
        )
        .unwrap();
        let content = Container::new(
            &mut ctx,
            LayoutStyle::new().flex_column().width(400.0).height(1000.0),
            vec![Box::new(btn), Box::new(txt)],
        )
        .unwrap();
        let content_node = content.layout_node();
        let sa = ScrollArea::new(
            &ctx,
            || Rect::new(0.0, 0.0, 400.0, 300.0),
            Box::new(content),
        );
        compute_layout(
            &mut ctx,
            content_node,
            AvailableSpace::Definite(400.0),
            AvailableSpace::MaxContent,
        )
        .unwrap();
        let br = track_layout(&ctx, btn_node).unwrap().get();

        let mut tree = crate::ComponentList::new(sa);
        let _ = tree.commands();

        // The button fires on release (tap), so send press then release.
        let cx = (br.x + br.width / 2.0) as f64;
        let cy = (br.y + br.height / 2.0) as f64;
        for phase in [true, false] {
            begin_batch();
            let ev = if phase {
                Event::PointerPressed {
                    x: cx,
                    y: cy,
                    button: PointerButton::Primary,
                    source: PointerSource::Mouse,
                }
            } else {
                Event::PointerReleased {
                    x: cx,
                    y: cy,
                    button: PointerButton::Primary,
                    source: PointerSource::Mouse,
                }
            };
            if tree.on_event(&ev) == EventResult::Handled {
                tree.bump_force_ticks();
                end_batch();
                begin_batch();
            }
            let _ = tree.commands();
            end_batch();
        }
        assert_eq!(
            s.get(),
            1,
            "scroll-content click should increment the signal"
        );
    }

    // A scroll gesture that begins on a button (touch-down, drag past the slop, release) must scroll the
    // content and NOT click the button — the scroll area cancels the pending tap once it detects the scroll.
    #[test]
    fn scroll_gesture_over_button_does_not_click() {
        use crate::button::Button;
        use crate::container::Container;
        use crate::context::track_layout;
        use platform_core::PointerButton;
        use reactive_core::signal;

        let mut ctx = WidgetCtx::new();
        let s = signal(0i32);
        let s_cb = s.clone();
        let btn = Button::new(&mut ctx, "x").unwrap();
        let btn_node = btn.layout_node();
        let btn = btn.on_click(move || s_cb.update(|n| *n += 1));
        let content = Container::new(
            &mut ctx,
            LayoutStyle::new().flex_column().width(400.0).height(1000.0),
            vec![Box::new(btn)],
        )
        .unwrap();
        let content_node = content.layout_node();
        let mut sa = ScrollArea::new(
            &ctx,
            || Rect::new(0.0, 0.0, 400.0, 300.0),
            Box::new(content),
        );
        compute_layout(
            &mut ctx,
            content_node,
            AvailableSpace::Definite(400.0),
            AvailableSpace::MaxContent,
        )
        .unwrap();
        let br = track_layout(&ctx, btn_node).unwrap().get();
        let (cx, cy) = (
            (br.x + br.width / 2.0) as f64,
            (br.y + br.height / 2.0) as f64,
        );

        // Touch-down on the button, then a drag: on Android each move sends Scrolled + PointerMoved.
        sa.on_event(&Event::PointerPressed {
            x: cx,
            y: cy,
            button: PointerButton::Primary,
            source: PointerSource::Touch { id: 1 },
        });
        for _ in 0..5 {
            sa.on_event(&Event::Scrolled {
                delta: ScrollDelta::Pixels { x: 0.0, y: -20.0 },
            });
            sa.on_event(&Event::PointerMoved {
                x: cx,
                y: cy,
                source: PointerSource::Touch { id: 1 },
            });
        }
        sa.on_event(&Event::PointerReleased {
            x: cx,
            y: cy,
            button: PointerButton::Primary,
            source: PointerSource::Touch { id: 1 },
        });

        assert_eq!(
            s.get(),
            0,
            "a scroll gesture over a button must not click it"
        );
        assert!(
            sa.core.scroll_y.get() > 0.0,
            "the gesture should have scrolled the content"
        );
    }

    #[test]
    fn as_layout_item_uses_leaf_rect_as_viewport() {
        let mut ctx = WidgetCtx::new();
        let ctx = &mut ctx;
        let content = Canvas::new(ctx, LayoutStyle::new().width(400.0).height(1000.0), |_| {
            RenderNode::Empty
        })
        .unwrap();
        let content_node = content.layout_node();
        let sa = LayoutScrollArea::new(
            ctx,
            LayoutStyle::new().width(400.0).height(300.0),
            Box::new(content),
        )
        .unwrap();
        let root = new_container(
            ctx,
            LayoutStyle::new().flex_column().width(400.0).height(300.0),
            &[sa.layout_node()],
        )
        .unwrap();
        compute_layout(
            ctx,
            root,
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(300.0),
        )
        .unwrap();
        compute_layout(
            ctx,
            content_node,
            AvailableSpace::Definite(400.0),
            AvailableSpace::MaxContent,
        )
        .unwrap();
        let vp = sa.viewport_rect();
        assert_eq!(vp.width, 400.0);
        assert_eq!(vp.height, 300.0);
    }

    #[test]
    fn as_layout_item_emits_clip_and_vbar_on_overflow() {
        let mut ctx = WidgetCtx::new();
        let ctx = &mut ctx;
        let content = Canvas::new(ctx, LayoutStyle::new().width(400.0).height(1000.0), |_| {
            RenderNode::Empty
        })
        .unwrap();
        let content_node = content.layout_node();
        let sa = LayoutScrollArea::new(
            ctx,
            LayoutStyle::new().width(400.0).height(300.0),
            Box::new(content),
        )
        .unwrap();
        let root = new_container(
            ctx,
            LayoutStyle::new().flex_column().width(400.0).height(300.0),
            &[sa.layout_node()],
        )
        .unwrap();
        compute_layout(
            ctx,
            root,
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(300.0),
        )
        .unwrap();
        compute_layout(
            ctx,
            content_node,
            AvailableSpace::Definite(400.0),
            AvailableSpace::MaxContent,
        )
        .unwrap();
        if let RenderNode::Group { children, .. } = sa.view() {
            assert_eq!(children.len(), 3);
            assert!(matches!(&children[0], RenderNode::Clip { .. }));
            assert!(matches!(
                &children[1],
                RenderNode::Primitive(DrawCommand::Rect { .. })
            ));
        } else {
            panic!("expected Group");
        }
    }

    #[test]
    fn scroll_lines_updates_offset() {
        let mut sa = make_scroll_area();
        sa.on_event(&Event::Scrolled {
            delta: ScrollDelta::Lines { x: 0.0, y: -3.0 },
        });
        assert_eq!(sa.core.scroll_y.get(), 60.0);
    }

    // Nested scroll: a wheel event is ignored when the pointer is outside this viewport (it belongs to an
    // ancestor, or to an inner scroll that already consumed it), so the outer area does not steal it.
    #[test]
    fn wheel_outside_viewport_does_not_scroll() {
        let mut sa = make_scroll_area(); // viewport 400x300
        sa.on_event(&Event::PointerMoved {
            x: 500.0,
            y: 500.0,
            source: PointerSource::Mouse,
        });
        let result = sa.on_event(&Event::Scrolled {
            delta: ScrollDelta::Lines { x: 0.0, y: -3.0 },
        });
        assert_eq!(result, EventResult::Ignored);
        assert_eq!(
            sa.core.scroll_y.get(),
            0.0,
            "must not scroll when the pointer is elsewhere"
        );
    }

    #[test]
    fn wheel_inside_viewport_scrolls() {
        let mut sa = make_scroll_area();
        sa.on_event(&Event::PointerMoved {
            x: 100.0,
            y: 100.0,
            source: PointerSource::Mouse,
        });
        sa.on_event(&Event::Scrolled {
            delta: ScrollDelta::Lines { x: 0.0, y: -3.0 },
        });
        assert!(
            sa.core.scroll_y.get() > 0.0,
            "wheel over the viewport scrolls it"
        );
    }

    #[test]
    fn scroll_pixels_updates_offset() {
        let mut sa = make_scroll_area();
        sa.on_event(&Event::Scrolled {
            delta: ScrollDelta::Pixels { x: 0.0, y: -80.0 },
        });
        assert_eq!(sa.core.scroll_y.get(), 80.0);
    }

    #[test]
    fn scroll_clamps_to_max() {
        let mut sa = make_scroll_area();
        sa.on_event(&Event::Scrolled {
            delta: ScrollDelta::Pixels { x: 0.0, y: -9999.0 },
        });
        assert_eq!(sa.core.scroll_y.get(), 700.0);
    }

    #[test]
    fn scroll_clamps_to_zero() {
        let mut sa = make_scroll_area();
        sa.on_event(&Event::Scrolled {
            delta: ScrollDelta::Pixels { x: 0.0, y: 9999.0 },
        });
        assert_eq!(sa.core.scroll_y.get(), 0.0);
    }

    #[test]
    fn pointer_outside_viewport_is_ignored() {
        let mut sa = make_scroll_area();
        let result = sa.on_event(&Event::PointerMoved {
            x: 500.0,
            y: 100.0,
            source: PointerSource::Mouse,
        });
        assert!(matches!(result, EventResult::Ignored));
    }

    #[test]
    fn view_emits_clip_and_scrollbar_when_content_overflows() {
        let sa = make_scroll_area();
        let view = sa.view();
        if let RenderNode::Group { children, .. } = view {
            assert_eq!(children.len(), 3);
            assert!(matches!(&children[0], RenderNode::Clip { .. }));
            assert!(matches!(
                &children[1],
                RenderNode::Primitive(DrawCommand::Rect { .. })
            ));
        } else {
            panic!("expected Group");
        }
    }

    #[test]
    fn view_no_scrollbar_when_content_fits() {
        let sa = make_scroll_area_small();
        let view = sa.view();
        if let RenderNode::Group { children, .. } = view {
            assert!(matches!(&children[1], RenderNode::Empty));
        } else {
            panic!("expected Group");
        }
    }

    #[test]
    fn child_receives_offset_pointer_event() {
        use std::cell::Cell;
        use std::rc::Rc;

        let captured_y: Rc<Cell<f64>> = Rc::new(Cell::new(-1.0));
        let captured_y_clone = captured_y.clone();

        struct CapturingItem {
            leaf: LayoutLeaf,
            out: Rc<Cell<f64>>,
        }
        impl Component for CapturingItem {
            fn view(&self) -> RenderNode {
                RenderNode::Empty
            }
            fn on_event(&mut self, event: &Event) -> EventResult {
                if let Event::PointerMoved { y, .. } = event {
                    self.out.set(*y);
                    EventResult::Handled
                } else {
                    EventResult::Ignored
                }
            }
        }
        impl LayoutItem for CapturingItem {
            fn layout_node(&self) -> NodeId {
                self.leaf.node
            }
        }

        let mut ctx = WidgetCtx::new();
        let ctx = &mut ctx;
        let leaf =
            LayoutLeaf::register(ctx, LayoutStyle::new().width(400.0).height(1000.0)).unwrap();
        let node = leaf.node;
        let content = CapturingItem {
            leaf,
            out: captured_y_clone,
        };
        let mut sa = ScrollArea::new(
            ctx,
            || Rect::new(100.0, 50.0, 400.0, 300.0),
            Box::new(content),
        );
        compute_layout(
            ctx,
            node,
            AvailableSpace::Definite(400.0),
            AvailableSpace::MaxContent,
        )
        .unwrap();

        sa.on_event(&Event::Scrolled {
            delta: ScrollDelta::Pixels { x: 0.0, y: -100.0 },
        });

        sa.on_event(&Event::PointerMoved {
            x: 150.0,
            y: 200.0,
            source: PointerSource::Mouse,
        });

        assert!((captured_y.get() - 250.0).abs() < 0.001);
    }

    fn make_scroll_area_wide() -> ScrollArea {
        let mut ctx = WidgetCtx::new();
        let ctx = &mut ctx;
        let content = Canvas::new(ctx, LayoutStyle::new().width(1000.0).height(300.0), |_| {
            RenderNode::Empty
        })
        .unwrap();
        let node = content.layout_node();
        let sa = ScrollArea::new(ctx, || Rect::new(0.0, 0.0, 400.0, 300.0), Box::new(content));
        compute_layout(
            ctx,
            node,
            AvailableSpace::Definite(1000.0),
            AvailableSpace::MaxContent,
        )
        .unwrap();
        sa
    }

    #[test]
    fn scroll_x_lines_updates_offset() {
        let mut sa = make_scroll_area_wide();
        sa.on_event(&Event::Scrolled {
            delta: ScrollDelta::Lines { x: -3.0, y: 0.0 },
        });
        assert_eq!(sa.core.scroll_x.get(), 60.0);
    }

    #[test]
    fn scroll_x_pixels_updates_offset() {
        let mut sa = make_scroll_area_wide();
        sa.on_event(&Event::Scrolled {
            delta: ScrollDelta::Pixels { x: -80.0, y: 0.0 },
        });
        assert_eq!(sa.core.scroll_x.get(), 80.0);
    }

    #[test]
    fn scroll_x_clamps_to_max() {
        let mut sa = make_scroll_area_wide();
        sa.on_event(&Event::Scrolled {
            delta: ScrollDelta::Pixels { x: -9999.0, y: 0.0 },
        });
        assert_eq!(sa.core.scroll_x.get(), 600.0);
    }

    #[test]
    fn scroll_x_clamps_to_zero() {
        let mut sa = make_scroll_area_wide();
        sa.on_event(&Event::Scrolled {
            delta: ScrollDelta::Pixels { x: 9999.0, y: 0.0 },
        });
        assert_eq!(sa.core.scroll_x.get(), 0.0);
    }

    #[test]
    fn view_emits_hbar_when_content_overflows_x() {
        let sa = make_scroll_area_wide();
        let view = sa.view();
        if let RenderNode::Group { children, .. } = view {
            assert_eq!(children.len(), 3);
            assert!(matches!(&children[0], RenderNode::Clip { .. }));
            assert!(matches!(&children[1], RenderNode::Empty));
            assert!(matches!(
                &children[2],
                RenderNode::Primitive(DrawCommand::Rect { .. })
            ));
        } else {
            panic!("expected Group");
        }
    }

    #[test]
    fn view_no_hbar_when_content_fits_x() {
        let sa = make_scroll_area();
        let view = sa.view();
        if let RenderNode::Group { children, .. } = view {
            assert_eq!(children.len(), 3);
            assert!(matches!(&children[2], RenderNode::Empty));
        } else {
            panic!("expected Group");
        }
    }
}
