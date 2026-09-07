//! The scroll viewport: offsets, scrollbars, the fling, and the content laid out as its own root.

use std::cell::RefCell;
use std::rc::Rc;

use geometry_core::Rect;
use layout_core::{AvailableSpace, LayoutError, LayoutStyle, NodeId};
use platform_core::Event;
use reactive_core::{Effect, ReadSignal, RwSignal, effect, signal};
use renderer_core::{BorderRadius, Color, RectStyle, ShapeStyle};
use theme_core::use_theme_tokens;
use ui_tree::{Component, EventResult, RenderNode, Segment};

use crate::context::track_layout;
use crate::impl_leaf_widget;
use crate::kept::kept;
use crate::layout_item::{LayoutItem, mount_item_segment};
use crate::layout_leaf::LayoutLeaf;
use crate::pointer::{clip_pointer_event, offset_pointer};
use crate::scroll_region::{ScrollRegionId, register_scroll_region, unregister_scroll_region};

/// How a scroll area's bars are painted, and how wide they are.
pub struct ScrollbarStyle {
    pub color: Color,
    pub width: f32,
    pub corner_radius: f32,
}

impl Default for ScrollbarStyle {
    fn default() -> Self {
        let color = use_theme_tokens().scrollbar();
        Self {
            color,
            width: 8.0,
            corner_radius: 3.0,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Axis {
    Vertical,
    Horizontal,
}

/// Shortest a thumb is allowed to get, so a very long document still leaves something to take hold of.
const MIN_THUMB: f32 = 24.0;

/// Where a scrollbar's thumb sits along one axis, and the room it has to travel.
///
/// One description read by both the drawing and the pointer: a thumb hit-tested against geometry worked out a second time is a thumb that can be drawn in one place and grabbed in another.
struct Thumb {
    start: f32,
    length: f32,
    travel: f32,
    max_scroll: f32,
}

impl Thumb {
    /// `None` when the content fits, which is when there is no bar to draw or to grab.
    fn of(origin: f32, viewport: f32, content: f32, scroll: f32) -> Option<Self> {
        if content <= viewport {
            return None;
        }
        let length = (viewport / content * viewport).max(MIN_THUMB);
        let max_scroll = (content - viewport).max(1.0);
        let travel = (viewport - length).max(0.0);
        Some(Self {
            start: origin + (scroll / max_scroll) * travel,
            length,
            travel,
            max_scroll,
        })
    }

    fn holds(&self, at: f32) -> bool {
        at >= self.start && at < self.start + self.length
    }

    /// The offset that would put the thumb's near edge at `start`.
    fn scroll_for(&self, origin: f32, start: f32) -> f32 {
        if self.travel <= 0.0 {
            return 0.0;
        }
        ((start - origin) / self.travel * self.max_scroll).clamp(0.0, self.max_scroll)
    }
}

fn draw_scrollbars(
    viewport: Rect,
    scroll_x: f32,
    scroll_y: f32,
    content_rect: Rect,
    scrollbar_style: &ScrollbarStyle,
) -> (RenderNode, RenderNode) {
    let style = || {
        RectStyle::default()
            .with_fill(scrollbar_style.color)
            .with_radius(BorderRadius::all(scrollbar_style.corner_radius))
    };

    let vbar = Thumb::of(viewport.y, viewport.height, content_rect.height, scroll_y)
        .map(|thumb| {
            RenderNode::rect(
                Rect::new(
                    viewport.x + viewport.width - scrollbar_style.width,
                    thumb.start,
                    scrollbar_style.width - 2.0,
                    thumb.length,
                ),
                style(),
            )
        })
        .unwrap_or(RenderNode::Empty);

    let hbar = Thumb::of(viewport.x, viewport.width, content_rect.width, scroll_x)
        .map(|thumb| {
            RenderNode::rect(
                Rect::new(
                    thumb.start,
                    viewport.y + viewport.height - scrollbar_style.width,
                    thumb.length,
                    scrollbar_style.width - 2.0,
                ),
                style(),
            )
        })
        .unwrap_or(RenderNode::Empty);

    (vbar, hbar)
}

/// How one scroll area's offset is moving other than by being set outright.
#[derive(Default)]
struct Motion {
    /// The wheel notches still being covered, one per axis.
    glide_x: Option<Rc<crate::fling::Glide>>,
    glide_y: Option<Rc<crate::fling::Glide>>,
    /// What the gesture in progress is doing, so its end can carry it on.
    velocity: crate::fling::Velocity,
}

impl Motion {
    /// Ends both. Whatever is taking the offset over is now the one that says where it goes, and a glide finishing afterwards would drag it back to where the wheel had asked for.
    fn stop_glides(&mut self) {
        for glide in [self.glide_x.take(), self.glide_y.take()]
            .into_iter()
            .flatten()
        {
            glide.stop();
        }
    }
}

fn glide_axis(
    slot: &mut Option<Rc<crate::fling::Glide>>,
    offset: RwSignal<f32>,
    delta: f32,
    bounds: (f32, f32),
) {
    if delta == 0.0 {
        return;
    }
    if let Some(glide) = slot.as_ref()
        && glide.extend(delta, bounds)
    {
        return;
    }
    *slot = crate::fling::Glide::start(offset, offset.peek() + delta, bounds);
}

fn handle_scroll_event(
    event: &Event,
    viewport: Rect,
    scroll_x: RwSignal<f32>,
    scroll_y: RwSignal<f32>,
    content_rect_signal: RwSignal<Rect>,
    content: &Rc<RefCell<Box<dyn LayoutItem>>>,
    motion: &mut Motion,
) -> EventResult {
    if let Event::Scrolled { delta, x, y } = event {
        // A surface that scrolls for itself has already decided which box the wheel moved and says so with `BoxScrolled`; answering here too would move the offset twice for one turn.
        if ui_tree::element_capture() {
            return EventResult::Ignored;
        }
        // Outside this viewport the wheel is an ancestor's to handle.
        if !viewport.contains(*x as f32, *y as f32) {
            return EventResult::Ignored;
        }
        // Offered to the content first, in content space, so an inner scroll area under the pointer consumes it first.
        let (inner_x, inner_y) =
            crate::scroll_region::snapped_offset(scroll_x.get(), scroll_y.get());
        let inner = offset_pointer(
            event,
            viewport.x as f64 - inner_x as f64,
            viewport.y as f64 - inner_y as f64,
        );
        if content
            .borrow_mut()
            .on_event(inner.as_ref().unwrap_or(event))
            == EventResult::Handled
        {
            return EventResult::Handled;
        }
        let (delta_x, delta_y) = delta.pixels();
        // Here rather than where the event arrived: this is where the scroll is known to be this area's. Recorded earlier, an outer area built up the speed of a gesture its content had consumed.
        motion.velocity.record(delta_y);
        let content_rect = content_rect_signal.get();
        let max_scroll_x = (content_rect.width - viewport.width).max(0.0);
        let max_scroll_y = (content_rect.height - viewport.height).max(0.0);
        // A notch says how far, never how fast, so it is the one scroll worth easing across. A trackpad and a finger report pixels already travelled, and animating those would animate a movement that has happened.
        if ui_tree::smooth_wheel() && matches!(delta, platform_core::ScrollDelta::Lines { .. }) {
            glide_axis(&mut motion.glide_x, scroll_x, -delta_x, (0.0, max_scroll_x));
            glide_axis(&mut motion.glide_y, scroll_y, -delta_y, (0.0, max_scroll_y));
        } else {
            scroll_x.set((scroll_x.get() - delta_x).clamp(0.0, max_scroll_x));
            scroll_y.set((scroll_y.get() - delta_y).clamp(0.0, max_scroll_y));
        }
        return EventResult::Handled;
    }

    let Some(event) = clip_pointer_event(event, viewport) else {
        return EventResult::Ignored;
    };

    let (snapped_x, snapped_y) =
        crate::scroll_region::snapped_offset(scroll_x.get(), scroll_y.get());
    let scroll_offset_x = snapped_x as f64;
    let scroll_offset_y = snapped_y as f64;
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
    // Shared between event dispatch (borrow_mut) and the content segment (borrow). The content is its own segment, so a scroll tick rewrites the transform without re-flattening it.
    content: Rc<RefCell<Box<dyn LayoutItem>>>,
    content_segment: Rc<Segment>,
    scrollbar_style: ScrollbarStyle,
    // Finger travel accumulated while a pointer is pressed. Content children see coords pinned under the finger and cannot detect the scroll themselves, so past `SCROLL_TAP_SLOP` the area cancels their pending tap. Gated on `press_active`, so a wheel scroll never cancels anything.
    press_active: bool,
    gesture_scroll: f32,
    tap_cancelled: bool,
    /// The one carrying on right now, if any. Held so a hand on the screen can stop it.
    fling: Option<std::rc::Rc<crate::fling::Fling>>,
    motion: Motion,
    /// The bar a pointer has hold of, and where along the thumb it took hold.
    bar_drag: Option<(Axis, f32)>,
    /// An offset this widget is *asking* for, on a surface that holds the content itself.
    ///
    /// There the offset travels the other way almost always: the compositor scrolls, and `BoxScrolled` tells this widget where the content ended up. Moving the signal alone would be overruled by the very next offset the surface reports — which is why the bar could not be dragged there at all. So it is published instead, on the element, and the backend puts the box where this says it should be.
    ///
    /// A `Cell`, because `view` is where it is handed over and `view` takes `&self`.
    commanded: std::cell::Cell<Option<(f32, f32)>>,
}

/// Accumulated finger travel (logical px) within a gesture past which the scroll area treats it as a scroll and cancels any pending tap on its content.
const SCROLL_TAP_SLOP: f32 = 8.0;

impl ScrollCore {
    /// Adopts externally-created scroll offset signals, so a caller can hand those same signals to the content it builds (see [`LayoutScrollArea::new_with`]). Fresh signals give an independent scroll.
    fn with_offsets(
        content_rect_signal: RwSignal<Rect>,
        content: Box<dyn LayoutItem>,
        scroll_x: RwSignal<f32>,
        scroll_y: RwSignal<f32>,
    ) -> Self {
        let content = Rc::new(RefCell::new(content));
        let content_segment = mount_item_segment(Rc::clone(&content));
        Self {
            content_rect_signal,
            scroll_x,
            scroll_y,
            content,
            content_segment,
            scrollbar_style: ScrollbarStyle::default(),
            press_active: false,
            gesture_scroll: 0.0,
            tap_cancelled: false,
            fling: None,
            motion: Motion::default(),
            bar_drag: None,
            commanded: std::cell::Cell::new(None),
        }
    }

    /// Puts an offset where this widget wants it, wherever the content actually is.
    ///
    /// The signal is the whole of it on a target that draws the content at the offset. Where the surface holds the content instead, the signal only *records* where the surface put it, so the surface has to be asked as well — see [`ScrollCore::commanded`].
    fn command(&self, x: f32, y: f32) {
        if self.scroll_x.peek() != x {
            self.scroll_x.set(x);
        }
        if self.scroll_y.peek() != y {
            self.scroll_y.set(y);
        }
        if ui_tree::element_capture() {
            self.commanded.set(Some((x, y)));
        }
    }

    /// The offset this widget is asking the surface for, taken by the frame that carries it.
    ///
    /// One frame, one request: a scroll a backend applies is applied at once, and one it does not — because the box cannot go that far — must not be asked for again, or every later frame would drag the box back to it and nothing else could scroll at all.
    fn take_command(&self) -> Option<(f32, f32)> {
        self.commanded.take()
    }

    /// [`ScrollCore::command`] for one axis, leaving the other where it is.
    fn command_axis(&self, axis: Axis, to: f32) {
        match axis {
            Axis::Vertical => self.command(self.scroll_x.peek(), to),
            Axis::Horizontal => self.command(to, self.scroll_y.peek()),
        }
    }

    // Both write the offsets they read, so both `peek` them: a caller may be inside an effect, where a reactive read would subscribe that effect to its own correction.
    fn scroll_to_top(&mut self) {
        // Whatever was still gliding was gliding through the page being left.
        self.catch_all();
        self.command(0.0, 0.0);
    }

    /// Takes the offset a surface that scrolls for itself has already applied.
    ///
    /// `peek` on both, and not for tidiness: this runs while the app is dispatching, and a reactive read here would subscribe whatever is running to an offset it is about to be told again.
    fn follow(&mut self, x: f32, y: f32) {
        // Where the content is beats where this widget was about to ask it to be: a fling is a scroll reported a frame late, and a stale request stops it dead.
        self.commanded.set(None);
        if self.scroll_x.peek() != x {
            self.scroll_x.set(x);
        }
        if self.scroll_y.peek() != y {
            self.scroll_y.set(y);
        }
    }

    /// Ends any fling where it stands.
    fn catch_fling(&mut self) {
        if let Some(fling) = self.fling.take() {
            fling.stop();
        }
    }

    /// Stops everything moving on its own. What a hand on the screen does, and what a jump to an offset somebody asked for does.
    fn catch_all(&mut self) {
        self.catch_fling();
        self.motion.stop_glides();
    }

    /// Takes hold of a bar under `(x, y)`, or pages towards a press on its track. `false` when the press landed somewhere else and belongs to the content.
    fn grab_bar(&mut self, viewport: Rect, x: f32, y: f32) -> bool {
        if !viewport.contains(x, y) {
            return false;
        }
        let content = self.content_rect_signal.get();
        let width = self.scrollbar_style.width;
        let on_vertical = x >= viewport.x + viewport.width - width;
        let on_horizontal = y >= viewport.y + viewport.height - width;

        // The vertical bar wins the corner they share, matching where it is drawn.
        let (axis, along, origin, extent, content_extent, offset) = if on_vertical {
            (
                Axis::Vertical,
                y,
                viewport.y,
                viewport.height,
                content.height,
                self.scroll_y,
            )
        } else if on_horizontal {
            (
                Axis::Horizontal,
                x,
                viewport.x,
                viewport.width,
                content.width,
                self.scroll_x,
            )
        } else {
            return false;
        };

        let Some(thumb) = Thumb::of(origin, extent, content_extent, offset.get()) else {
            return false;
        };
        self.catch_all();
        if thumb.holds(along) {
            self.bar_drag = Some((axis, along - thumb.start));
        } else {
            let by = if along < thumb.start { -extent } else { extent };
            self.command_axis(axis, (offset.get() + by).clamp(0.0, thumb.max_scroll));
        }
        true
    }

    fn drag_bar(&mut self, viewport: Rect, x: f32, y: f32) {
        let Some((axis, grab)) = self.bar_drag else {
            return;
        };
        let content = self.content_rect_signal.get();
        let (along, origin, extent, content_extent, offset) = match axis {
            Axis::Vertical => (
                y,
                viewport.y,
                viewport.height,
                content.height,
                self.scroll_y,
            ),
            Axis::Horizontal => (x, viewport.x, viewport.width, content.width, self.scroll_x),
        };
        if let Some(thumb) = Thumb::of(origin, extent, content_extent, offset.get()) {
            self.command_axis(axis, thumb.scroll_for(origin, along - grab));
        }
    }

    /// Carries the gesture on, if it was still going when it ended.
    fn launch_fling(&mut self, viewport: Rect) {
        // A surface that scrolls for itself brings its own physics, and two would fight over one offset.
        if ui_tree::element_capture() {
            self.motion.velocity.clear();
            return;
        }
        // The offset grows as the content moves up, the opposite sign to the gesture.
        let velocity = -self.motion.velocity.take();
        let max = (self.content_rect_signal.peek().height - viewport.height).max(0.0);
        self.fling = crate::fling::Fling::start(self.scroll_y, velocity, (0.0, max));
    }

    fn clamp_scroll(&mut self, viewport: Rect) {
        // The content resized under it, so the ground it was travelling over is gone.
        self.catch_all();
        let content_rect = self.content_rect_signal.peek();
        let max_x = (content_rect.width - viewport.width).max(0.0);
        let max_y = (content_rect.height - viewport.height).max(0.0);
        let clamped_x = self.scroll_x.peek().clamp(0.0, max_x);
        let clamped_y = self.scroll_y.peek().clamp(0.0, max_y);
        if self.scroll_x.peek() != clamped_x {
            self.scroll_x.set(clamped_x);
        }
        if self.scroll_y.peek() != clamped_y {
            self.scroll_y.set(clamped_y);
        }
    }

    fn view(&self, viewport: Rect) -> RenderNode {
        let scroll_x = self.scroll_x.get();
        let scroll_y = self.scroll_y.get();
        let content_rect = self.content_rect_signal.get();
        // Nowhere, on a target that scrolls for itself: it has already moved the content, and displacing it again would scroll it twice. The offset is still held, because hit-testing, anchored overlays and `visible_rect` all read it.
        let owns_scroll = ui_tree::element_capture();
        let (dx, dy) = if owns_scroll {
            (0.0, 0.0)
        } else {
            let (sx, sy) = crate::scroll_region::snapped_offset(scroll_x, scroll_y);
            (viewport.x - sx, viewport.y - sy)
        };
        let scrollable = RenderNode::clip(
            viewport,
            BorderRadius::zero(),
            [RenderNode::translate(
                dx,
                dy,
                [self.content_segment.boundary()],
            )],
        );
        // Telar's bar on every target: a native one takes width out of the box and layout never reserved it. Where the surface scrolls, the content moves under a bar that must not, so it is drawn at the content's offset and comes out standing still.
        let bar_viewport = if owns_scroll {
            Rect::new(scroll_x, scroll_y, viewport.width, viewport.height)
        } else {
            viewport
        };
        let (vbar, hbar) = draw_scrollbars(
            bar_viewport,
            scroll_x,
            scroll_y,
            content_rect,
            &self.scrollbar_style,
        );
        RenderNode::group([scrollable, vbar, hbar])
    }

    fn on_event(&mut self, event: &Event, viewport: Rect) -> EventResult {
        match event {
            // A press on a scrollbar is the bar's, not the content's: a button under the thumb must not take the click that grabbed it.
            Event::PointerPressed { x, y, .. } if self.grab_bar(viewport, *x as f32, *y as f32) => {
                return EventResult::Handled;
            }
            Event::PointerMoved { x, y, .. } if self.bar_drag.is_some() => {
                self.drag_bar(viewport, *x as f32, *y as f32);
                return EventResult::Handled;
            }
            Event::PointerReleased { .. } if self.bar_drag.is_some() => {
                self.bar_drag = None;
                return EventResult::Handled;
            }
            // A new press starts a fresh tap candidate; forget the prior gesture's accumulated scroll.
            Event::PointerPressed { .. } => {
                self.press_active = true;
                self.gesture_scroll = 0.0;
                self.tap_cancelled = false;
                // A hand on the screen stops whatever was still moving, and stops it where it stands.
                self.catch_all();
                self.motion.velocity.clear();
            }
            // A drag that ends while still moving does not stop where it was let go of.
            Event::PointerReleased { .. } => {
                self.press_active = false;
                self.launch_fling(viewport);
            }
            // Anything scrolling now owns the offset, so what was coasting does not. This also keeps a platform running its own inertia from having Telar's added on top.
            Event::Scrolled { delta, .. } => {
                self.catch_fling();
                // While a pointer is down, once the finger passes the slop this gesture is a scroll and not a tap: cancel the content's pending press once, since it sees pinned coords and cannot tell on its own.
                if self.press_active && !self.tap_cancelled {
                    let (dx, dy) = delta.pixels();
                    self.gesture_scroll += (dx * dx + dy * dy).sqrt();
                    if self.gesture_scroll > SCROLL_TAP_SLOP {
                        self.tap_cancelled = true;
                        self.content.borrow_mut().on_event(&Event::CursorLeft);
                    }
                }
            }
            // Fingers off the touchpad. A touch-screen drag ends with a release instead, which is why both arms end here.
            Event::ScrollEnded { .. } => self.launch_fling(viewport),
            _ => {}
        }
        handle_scroll_event(
            event,
            viewport,
            self.scroll_x,
            self.scroll_y,
            self.content_rect_signal,
            &self.content,
            &mut self.motion,
        )
    }
}

// Closure-viewport fixture exercising ScrollCore directly; LayoutScrollArea is the only public scroll area.
#[cfg(test)]
struct ScrollArea {
    viewport: Box<dyn Fn() -> Rect>,
    core: ScrollCore,
}

#[cfg(test)]
impl ScrollArea {
    fn new(viewport: impl Fn() -> Rect + 'static, content: Box<dyn LayoutItem>) -> Self {
        let content_rect_signal =
            track_layout(content.layout_node()).expect("content node not registered in ctx");
        Self {
            viewport: Box::new(viewport),
            core: ScrollCore::with_offsets(content_rect_signal, content, signal(0.0), signal(0.0)),
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

/// A handle to the enclosing scroll area's live viewport, handed to the content builder by [`LayoutScrollArea::new_with`]. Because a scroll area lays its content out as its OWN layout root, every descendant's tracked rect is already in the same content-local space the scroll offset indexes into — so `visible` is a plain rect overlap, no scroll-transform math.
#[derive(Clone)]
pub struct ScrollViewport {
    offset_x: ReadSignal<f32>,
    offset_y: ReadSignal<f32>,
    rect: ReadSignal<Rect>,
    // The writable side of the same offsets, so `reveal` can move the view. Private: callers should say what they want visible, not compute a scroll position.
    set_x: RwSignal<f32>,
    set_y: RwSignal<f32>,
}

impl ScrollViewport {
    /// The live scroll offset `(x, y)` in content-local px.
    pub fn offset(&self) -> (ReadSignal<f32>, ReadSignal<f32>) {
        (self.offset_x, self.offset_y)
    }

    /// Scrolls the minimum distance needed to bring `item` fully into view, leaving `margin` px of breathing room at whichever edge it entered from. A no-op when the item is already visible.
    ///
    /// This is what keyboard navigation needs and what a scroll offset alone cannot express: moving a selection down a list should follow it, without yanking the view when the item was on screen all along. `item` must be a node inside this scroll's content, so its tracked rect shares the content-local space the offset indexes into.
    pub fn reveal(&self, item: NodeId, margin: f32) {
        let Some(item_rect) = track_layout(item) else {
            return;
        };
        let item = item_rect.get();
        let viewport = self.rect.get();

        let reveal_axis = |offset: f32, span: f32, start: f32, size: f32| -> f32 {
            if start - margin < offset {
                (start - margin).max(0.0)
            } else if start + size + margin > offset + span {
                (start + size + margin - span).max(0.0)
            } else {
                offset
            }
        };

        // `peek` is load-bearing here: the natural caller is an effect ("keep the selected row in view"), and a reactive read of the offset would subscribe it to the signal it writes, dragging the view back on every manual scroll. The item and viewport rects are inputs, so re-running when those move is correct.
        let y = reveal_axis(self.set_y.peek(), viewport.height, item.y, item.height);
        if y != self.set_y.peek() {
            self.set_y.set(y);
        }
        let x = reveal_axis(self.set_x.peek(), viewport.width, item.x, item.width);
        if x != self.set_x.peek() {
            self.set_x.set(x);
        }
    }

    /// The live viewport rect; its `width`/`height` are the visible window's size.
    pub fn rect(&self) -> ReadSignal<Rect> {
        self.rect
    }

    /// Puts the view back at the top-left.
    ///
    /// For content that has been *replaced* rather than resized — a page swapped for another one — which is the one thing the scroll area cannot tell on its own: a shorter page is clamped back into range automatically, but only the caller knows that what is in the viewport is now a different thing, and that being three screens down someone else's page is not where the reader left off.
    ///
    /// `peek` for the same reason as [`reveal`](Self::reveal), and this is where it bites hardest: "the page changed" is noticed by an effect, so a reactive read of the offset would make every wheel tick re-run the effect that puts the offset back — the viewport pinned to the top for good.
    pub fn scroll_to_top(&self) {
        if self.set_x.peek() != 0.0 {
            self.set_x.set(0.0);
        }
        if self.set_y.peek() != 0.0 {
            self.set_y.set(0.0);
        }
    }
}

/// Taffy-layout viewport; always valid as a `LayoutItem`, so no panic is possible.
pub struct LayoutScrollArea {
    leaf: LayoutLeaf,
    core: ScrollCore,
    // Publishes the offset so anything positioning against a node inside it can ask where that node is drawn rather than where it was laid out.
    scroll_region: ScrollRegionId,
    // The content is not a taffy child of the viewport leaf, so nothing else would lay it out; this re-lays the detached subtree whenever the viewport is resized.
    _layout_effect: Effect,
    // Keeps the offset inside the range the content and viewport currently allow.
    _clamp_effect: Effect,
}

impl LayoutScrollArea {
    pub fn new(
        layout_style: LayoutStyle,
        content: Box<dyn LayoutItem>,
    ) -> Result<Self, LayoutError> {
        Self::new_with(layout_style, move |_| Ok(content))
    }

    /// Like [`new`](Self::new), but the content is built with access to this scroll's live [`ScrollViewport`], so descendants can gate work (e.g. lazy asset loading) on whether they are currently on screen. The offset/viewport signals are created BEFORE `build` runs, so the content it returns can capture them — resolving the ordering bind where the scroll is built from its own content yet the content needs the scroll's signals.
    pub fn new_with<F>(layout_style: LayoutStyle, build: F) -> Result<Self, LayoutError>
    where
        F: FnOnce(ScrollViewport) -> Result<Box<dyn LayoutItem>, LayoutError>,
    {
        Self::new_keeping(layout_style, (signal(0.0), signal(0.0)), build)
    }

    /// A scroll area whose position the *surface* keeps under `key`, so it survives a rebuild of the tree.
    ///
    /// The usual spelling of [`new_keeping`](Self::new_keeping): a remounted view — a shell following a config edit, a page rebuilt under the same window — reopens where the reader left it instead of snapping to the top. `key` names this viewport among everything else the surface keeps, so two scroll areas on one surface need two keys (see [`kept`]).
    pub fn new_kept<F>(
        key: &'static str,
        layout_style: LayoutStyle,
        build: F,
    ) -> Result<Self, LayoutError>
    where
        F: FnOnce(ScrollViewport) -> Result<Box<dyn LayoutItem>, LayoutError>,
    {
        let offset = kept(key, || (signal(0.0f32), signal(0.0f32)));
        Self::new_keeping(layout_style, offset, build)
    }

    /// Like [`new_with`](Self::new_with), but against offset signals the *caller* owns — so the scroll position can outlive this widget.
    ///
    /// For a tree that is rebuilt while its surface stays (a shell following a config edit, a view remounted under the same window): a scroll area built with fresh signals starts at the top every time, which reads as the list jumping back under the reader's hands. Hand it the same pair on every build and the view is where they left it. [`new_kept`](Self::new_kept) is this with the surface holding the pair.
    pub fn new_keeping<F>(
        layout_style: LayoutStyle,
        offset: (RwSignal<f32>, RwSignal<f32>),
        build: F,
    ) -> Result<Self, LayoutError>
    where
        F: FnOnce(ScrollViewport) -> Result<Box<dyn LayoutItem>, LayoutError>,
    {
        let leaf = LayoutLeaf::register(layout_style)?;
        let (scroll_x, scroll_y) = offset;
        let content = build(ScrollViewport {
            offset_x: scroll_x.read_only(),
            offset_y: scroll_y.read_only(),
            rect: leaf.rect.read_only(),
            set_x: scroll_x,
            set_y: scroll_y,
        })?;
        let content_node = content.layout_node();
        let content_rect_signal =
            track_layout(content_node).expect("content node not registered in ctx");

        // The viewport rect is set by the surrounding layout and this effect fires during that flush, after the runtime borrow is released, so computing here is re-entrancy safe.
        let viewport = leaf.rect;
        let layout_effect = effect(move || {
            let vp = viewport.get();
            if vp.width > 0.0 {
                let _ = crate::context::compute_layout(
                    content_node,
                    AvailableSpace::Definite(vp.width),
                    AvailableSpace::MaxContent,
                );
            }
        });

        // Both things deciding where the end is move underneath the offset: the content's height and the viewport's. Clamped from an effect because neither is an input event — left to the next wheel tick, the transform pushes the content out of the clip and the viewport shows nothing until it is touched.
        let clamp_effect = {
            let viewport = leaf.rect;
            let content_rect = content_rect_signal;
            let (scroll_x, scroll_y) = (scroll_x, scroll_y);
            effect(move || {
                let vp = viewport.get();
                let content = content_rect.get();
                // A zero rect is "not laid out yet", not "empty": clamping against it would throw away an offset the caller kept, one flush before the layout that justifies it.
                if vp.height <= 0.0 || content.height <= 0.0 {
                    return;
                }
                let max_x = (content.width - vp.width).max(0.0);
                let max_y = (content.height - vp.height).max(0.0);
                // `peek`, not `get`: this effect writes those signals, and reading them would re-run it for its own correction.
                if scroll_x.peek() > max_x {
                    scroll_x.set(max_x);
                }
                if scroll_y.peek() > max_y {
                    scroll_y.set(max_y);
                }
            })
        };

        // On the content node, not the viewport leaf: the content is laid out as its own root, so the leaf is never its ancestor and a subtree test would miss.
        let scroll_region = register_scroll_region(content_node, scroll_x, scroll_y);

        Ok(Self {
            leaf,
            core: ScrollCore::with_offsets(content_rect_signal, content, scroll_x, scroll_y),
            scroll_region,
            _layout_effect: layout_effect,
            _clamp_effect: clamp_effect,
        })
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

impl Drop for LayoutScrollArea {
    fn drop(&mut self) {
        unregister_scroll_region(self.scroll_region);
    }
}

impl_leaf_widget!(LayoutScrollArea);

impl Component for LayoutScrollArea {
    fn view(&self) -> RenderNode {
        // Its own box rather than `LayoutLeaf::at_layout_position`: the content is placed by the scroll offset, not by the leaf's placement, so the two must not both apply.
        let content = self.core.view(self.leaf.rect.get());
        if ui_tree::element_capture() {
            let semantics = renderer_core::Semantics::of(renderer_core::Role::ScrollArea);
            let element = crate::element::with_semantics_scrolled(
                self.leaf.node,
                semantics,
                self.core.take_command(),
            );
            RenderNode::element(element, [content])
        } else {
            content
        }
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        // The surface already performed the scroll, so what needs correcting is this widget's idea of where the content is.
        if let Event::BoxScrolled { box_id, x, y } = event
            && *box_id == u64::from(self.leaf.node)
        {
            self.core.follow(*x, *y);
            return EventResult::Handled;
        }
        self.core.on_event(event, self.leaf.rect.get())
    }

    fn debug_name(&self) -> &'static str {
        "ScrollArea"
    }
}

#[cfg(test)]
#[path = "scroll_area_test.rs"]
mod tests;

#[cfg(test)]
#[path = "scroll_area_scrollbar_test.rs"]
mod scrollbar_tests;

#[cfg(test)]
#[path = "scroll_area_touchpad_test.rs"]
mod touchpad_tests;

/// The bar on a surface that holds the content itself — a document backend, where the compositor scrolls and this widget is told where the content ended up rather than deciding it.
#[cfg(test)]
#[path = "scroll_area_surface_scroll_test.rs"]
mod surface_scroll_tests;
