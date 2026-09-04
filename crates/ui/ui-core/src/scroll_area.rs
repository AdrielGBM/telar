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
/// One description read by both the drawing and the pointer: a thumb hit-tested against geometry worked out
/// a second time is a thumb that can be drawn in one place and grabbed in another.
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

/// The wheel notches still being covered, one per axis.
#[derive(Default)]
struct Glides {
    x: Option<Rc<crate::fling::Glide>>,
    y: Option<Rc<crate::fling::Glide>>,
}

impl Glides {
    /// Ends both. Whatever is taking the offset over is now the one that says where it goes, and a glide
    /// finishing afterwards would drag it back to where the wheel had asked for.
    fn stop(&mut self) {
        for glide in [self.x.take(), self.y.take()].into_iter().flatten() {
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
    glides: &mut Glides,
) -> EventResult {
    if let Event::Scrolled { delta, x, y } = event {
        // A surface that scrolls for itself has already decided which box the wheel moved, and says so with
        // `BoxScrolled`. Answering it here as well would move the offset twice for one turn.
        if ui_tree::element_capture() {
            return EventResult::Ignored;
        }
        // The wheel belongs to whatever is under it: outside this viewport it is an ancestor's to handle.
        if !viewport.contains(*x as f32, *y as f32) {
            return EventResult::Ignored;
        }
        // Nested scroll: offer the wheel to the content first (in content space, like every other pointer
        // event that crosses this boundary), so an inner scroll area under the pointer consumes it first.
        let inner = offset_pointer(
            event,
            viewport.x as f64 - scroll_x.get() as f64,
            viewport.y as f64 - scroll_y.get() as f64,
        );
        if content
            .borrow_mut()
            .on_event(inner.as_ref().unwrap_or(event))
            == EventResult::Handled
        {
            return EventResult::Handled;
        }
        let (delta_x, delta_y) = delta.pixels();
        let content_rect = content_rect_signal.get();
        let max_scroll_x = (content_rect.width - viewport.width).max(0.0);
        let max_scroll_y = (content_rect.height - viewport.height).max(0.0);
        // A notch says how far, never how fast, so it is the one kind of scroll worth easing across. A
        // trackpad and a finger report pixels they have already travelled, and animating those would be
        // animating a movement that has happened.
        if ui_tree::smooth_wheel() && matches!(delta, platform_core::ScrollDelta::Lines { .. }) {
            glide_axis(&mut glides.x, scroll_x, -delta_x, (0.0, max_scroll_x));
            glide_axis(&mut glides.y, scroll_y, -delta_y, (0.0, max_scroll_y));
        } else {
            scroll_x.set((scroll_x.get() - delta_x).clamp(0.0, max_scroll_x));
            scroll_y.set((scroll_y.get() - delta_y).clamp(0.0, max_scroll_y));
        }
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
    /// What the gesture in progress is doing, so letting go can carry it on.
    velocity: crate::fling::Velocity,
    /// The one carrying on right now, if any. Held so a hand on the screen can stop it.
    fling: Option<std::rc::Rc<crate::fling::Fling>>,
    glides: Glides,
    /// The bar a pointer has hold of, and where along the thumb it took hold.
    bar_drag: Option<(Axis, f32)>,
}

/// Accumulated finger travel (logical px) within a gesture past which the scroll area treats it as a scroll
/// and cancels any pending tap on its content.
const SCROLL_TAP_SLOP: f32 = 8.0;

impl ScrollCore {
    /// Adopts externally-created scroll offset signals, so a caller can hand those same signals to the
    /// content it builds (see [`LayoutScrollArea::new_with`]). Fresh signals give an independent scroll.
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
            velocity: crate::fling::Velocity::default(),
            fling: None,
            glides: Glides::default(),
            bar_drag: None,
        }
    }

    // Both of these write the offsets they read, so both `peek` them: whoever calls them may be doing so from
    // an effect, and a reactive read there would subscribe that effect to its own correction (see
    // `ScrollViewport::reveal`, where that bug bites).
    fn scroll_to_top(&mut self) {
        // Whatever was still gliding was gliding through the page being left.
        self.catch_fling();
        if self.scroll_x.peek() != 0.0 {
            self.scroll_x.set(0.0);
        }
        if self.scroll_y.peek() != 0.0 {
            self.scroll_y.set(0.0);
        }
    }

    /// Takes the offset a surface that scrolls for itself has already applied.
    ///
    /// `peek` on both, and not for tidiness: this runs while the app is dispatching, and a reactive read here
    /// would subscribe whatever is running to an offset it is about to be told again.
    fn follow(&mut self, x: f32, y: f32) {
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
        self.catch_glide();
    }

    fn catch_glide(&mut self) {
        self.glides.stop();
    }

    /// Takes hold of a bar under `(x, y)`, or pages towards a press on its track. `false` when the press
    /// landed somewhere else and belongs to the content.
    fn grab_bar(&mut self, viewport: Rect, x: f32, y: f32) -> bool {
        // A surface that scrolls for itself draws these bars pinned to a viewport it does not own the offset
        // of; moving that offset from here would be overruled by the next frame the surface reports.
        if ui_tree::element_capture() || !viewport.contains(x, y) {
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
        self.catch_fling();
        if thumb.holds(along) {
            self.bar_drag = Some((axis, along - thumb.start));
        } else {
            let by = if along < thumb.start { -extent } else { extent };
            offset.set((offset.get() + by).clamp(0.0, thumb.max_scroll));
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
            offset.set(thumb.scroll_for(origin, along - grab));
        }
    }

    /// Carries the gesture on, if it was still going when it ended.
    fn launch_fling(&mut self, viewport: Rect) {
        // A surface that scrolls for itself brings its own physics, and two would fight over one offset.
        if ui_tree::element_capture() {
            self.velocity.clear();
            return;
        }
        // The offset grows as the content moves *up*, which is the opposite sign to the gesture.
        let velocity = -self.velocity.take();
        let max = (self.content_rect_signal.peek().height - viewport.height).max(0.0);
        self.fling = crate::fling::Fling::start(self.scroll_y, velocity, (0.0, max));
    }

    fn clamp_scroll(&mut self, viewport: Rect) {
        // The content resized under it, so the ground it was travelling over is not there any more.
        self.catch_fling();
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
        // Where the content sits. Nowhere, on a target that scrolls for itself: it lays the content out at
        // full height inside a box that scrolls, and has already moved it — displacing it again here would
        // scroll it twice. The offset is still *held*, because hit-testing, anchored overlays and
        // `visible_rect` all read it; it is just no longer what puts the content where it is.
        let owns_scroll = ui_tree::element_capture();
        let (dx, dy) = if owns_scroll {
            (0.0, 0.0)
        } else {
            (viewport.x - scroll_x, viewport.y - scroll_y)
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
        // The bar is Telar's on every target, the one that scrolls itself included: a native bar takes width
        // out of the box it is in, and layout never reserved it — a rail came out fifteen pixels narrower
        // than every rect hit-testing reads. Where the surface scrolls, the content moves under a bar that
        // must not, so it is drawn at the offset the content is at and comes out standing still.
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
            // A press on a scrollbar is that bar's, not the content's: the bar is drawn over whatever is
            // beneath it, and a button under the thumb must not take the click that grabbed it.
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
                // A hand on the screen stops whatever was still moving, and stops it where it stands: that
                // is how a list is caught.
                self.catch_fling();
                self.velocity.clear();
            }
            // A drag that ends while still moving does not stop where it was let go of.
            Event::PointerReleased { .. } => {
                self.press_active = false;
                self.launch_fling(viewport);
            }
            // While a pointer is down (a touch drag, not a mouse wheel), once the finger has travelled past
            // the slop this gesture is a scroll, not a tap: cancel the pending press on the content once (it
            // sees pinned content-space coords and can't tell on its own).
            Event::Scrolled { delta, .. } if self.press_active => {
                let (dx, dy) = delta.pixels();
                // Every move of the gesture, not only the ones before it was declared a scroll: the estimate
                // has to still be warm when the finger lifts, and the arm below stops matching after the
                // first eight pixels.
                self.velocity.record(dy);
                if !self.tap_cancelled {
                    self.gesture_scroll += (dx * dx + dy * dy).sqrt();
                    if self.gesture_scroll > SCROLL_TAP_SLOP {
                        self.tap_cancelled = true;
                        self.content.borrow_mut().on_event(&Event::CursorLeft);
                    }
                }
            }
            _ => {}
        }
        handle_scroll_event(
            event,
            viewport,
            self.scroll_x,
            self.scroll_y,
            self.content_rect_signal,
            &self.content,
            &mut self.glides,
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

/// A handle to the enclosing scroll area's live viewport, handed to the content builder by
/// [`LayoutScrollArea::new_with`]. Because a scroll area lays its content out as its OWN layout root,
/// every descendant's tracked rect is already in the same content-local space the scroll offset
/// indexes into — so [`visible`](Self::visible) is a plain rect overlap, no scroll-transform math.
#[derive(Clone)]
pub struct ScrollViewport {
    offset_x: ReadSignal<f32>,
    offset_y: ReadSignal<f32>,
    rect: ReadSignal<Rect>,
    // The writable side of the same offsets, so `reveal` can move the view. Kept private: callers should say
    // *what they want visible*, not compute a scroll position.
    set_x: RwSignal<f32>,
    set_y: RwSignal<f32>,
}

impl ScrollViewport {
    /// The live scroll offset `(x, y)` in content-local px.
    pub fn offset(&self) -> (ReadSignal<f32>, ReadSignal<f32>) {
        (self.offset_x, self.offset_y)
    }

    /// Scrolls the minimum distance needed to bring `item` fully into view, leaving `margin` px of breathing
    /// room at whichever edge it entered from. A no-op when the item is already visible.
    ///
    /// This is what keyboard navigation needs and what a scroll offset alone cannot express: moving a selection
    /// down a list should follow it, without yanking the view when the item was on screen all along. `item` must
    /// be a node inside this scroll's content, so its tracked rect shares the content-local space the offset
    /// indexes into.
    pub fn reveal(&self, item: NodeId, margin: f32) {
        let Some(item_rect) = track_layout(item) else {
            return;
        };
        let item = item_rect.get();
        let viewport = self.rect.get();

        let reveal_axis = |offset: f32, span: f32, start: f32, size: f32| -> f32 {
            if start - margin < offset {
                // Entered from the near edge: put its leading edge at the top/left of the window.
                (start - margin).max(0.0)
            } else if start + size + margin > offset + span {
                // Entered from the far edge: put its trailing edge at the bottom/right.
                (start + size + margin - span).max(0.0)
            } else {
                offset
            }
        };

        // `peek` on the offsets, and it is load-bearing: this is a *command*, and the natural place to call it
        // from is an effect ("while this row is the selected one, keep it in view"). A reactive read of the
        // offset there would subscribe that effect to the very signal it writes, so scrolling by hand would
        // re-run it and drag the view straight back — a list that cannot be scrolled at all. The item and
        // viewport rects are inputs rather than outputs, so re-running when *those* move is the right thing.
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
    /// For content that has been *replaced* rather than resized — a page swapped for another one — which is
    /// the one thing the scroll area cannot tell on its own: a shorter page is clamped back into range
    /// automatically, but only the caller knows that what is in the viewport is now a different thing, and
    /// that being three screens down someone else's page is not where the reader left off.
    ///
    /// `peek` for the same reason as [`reveal`](Self::reveal), and this is where it bites hardest: "the page
    /// changed" is noticed by an effect, so a reactive read of the offset would make every wheel tick re-run
    /// the effect that puts the offset back — the viewport pinned to the top for good.
    pub fn scroll_to_top(&self) {
        if self.set_x.peek() != 0.0 {
            self.set_x.set(0.0);
        }
        if self.set_y.peek() != 0.0 {
            self.set_y.set(0.0);
        }
    }
}

// Taffy-layout viewport; always valid as a LayoutItem — no panic possible.
pub struct LayoutScrollArea {
    leaf: LayoutLeaf,
    core: ScrollCore,
    // Publishes this viewport's offset so anything positioning against a node inside it (an anchored dropdown's trigger) can ask where that node is drawn rather than where it was laid out.
    scroll_region: ScrollRegionId,
    // Lays the detached content subtree out against the viewport width whenever the viewport is (re)sized,
    // so a `scroll` element works on its own — its content is not a taffy child of the viewport leaf, so
    // nothing else would lay it out (the app shell computes only its OWN top-level scroll by hand).
    _layout_effect: Effect,
    // Keeps the offset inside the range the content and the viewport currently allow. See `clamp_effect`.
    _clamp_effect: Effect,
}

impl LayoutScrollArea {
    pub fn new(
        layout_style: LayoutStyle,
        content: Box<dyn LayoutItem>,
    ) -> Result<Self, LayoutError> {
        Self::new_with(layout_style, move |_| Ok(content))
    }

    /// Like [`new`](Self::new), but the content is built with access to this scroll's live
    /// [`ScrollViewport`], so descendants can gate work (e.g. lazy asset loading) on whether they are
    /// currently on screen. The offset/viewport signals are created BEFORE `build` runs, so the content
    /// it returns can capture them — resolving the ordering bind where the scroll is built from its own
    /// content yet the content needs the scroll's signals.
    pub fn new_with<F>(layout_style: LayoutStyle, build: F) -> Result<Self, LayoutError>
    where
        F: FnOnce(ScrollViewport) -> Result<Box<dyn LayoutItem>, LayoutError>,
    {
        Self::new_keeping(layout_style, (signal(0.0), signal(0.0)), build)
    }

    /// A scroll area whose position the *surface* keeps under `key`, so it survives a rebuild of the tree.
    ///
    /// The usual spelling of [`new_keeping`](Self::new_keeping): a remounted view — a shell following a
    /// config edit, a page rebuilt under the same window — reopens where the reader left it instead of
    /// snapping to the top. `key` names this viewport among everything else the surface keeps, so two scroll
    /// areas on one surface need two keys (see [`kept`]).
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

    /// Like [`new_with`](Self::new_with), but against offset signals the *caller* owns — so the scroll
    /// position can outlive this widget.
    ///
    /// For a tree that is rebuilt while its surface stays (a shell following a config edit, a view remounted
    /// under the same window): a scroll area built with fresh signals starts at the top every time, which
    /// reads as the list jumping back under the reader's hands. Hand it the same pair on every build and the
    /// view is where they left it. [`new_kept`](Self::new_kept) is this with the surface holding the pair.
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

        // Re-lay out the content at the viewport width (unbounded height, so it can overflow and scroll)
        // each time the viewport resizes. The viewport rect is set by the surrounding layout; this effect
        // fires during that flush — after the runtime borrow is released — so computing here is re-entrancy
        // safe (same pattern as reactive lists).
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

        // Nothing may be scrolled past the end of what there is, and *both* things that decide where the end
        // is move underneath the offset: how tall the content is (a page swapped for a shorter one, a list
        // that lost rows) and how tall the viewport is (a window resized). Clamped from an effect rather than
        // from the scroll handler because neither of those is an input event — left to the next wheel tick,
        // the transform goes on pushing the content clean out of the clip and the viewport shows *nothing*,
        // which is the shape this bug always takes: a page that is blank until it is touched.
        let clamp_effect = {
            let viewport = leaf.rect;
            let content_rect = content_rect_signal;
            let (scroll_x, scroll_y) = (scroll_x, scroll_y);
            effect(move || {
                let vp = viewport.get();
                let content = content_rect.get();
                // A zero rect is "not laid out yet", not "empty": clamping against it would throw away an
                // offset the caller deliberately kept, one flush before the layout that justifies it.
                if vp.height <= 0.0 || content.height <= 0.0 {
                    return;
                }
                let max_x = (content.width - vp.width).max(0.0);
                let max_y = (content.height - vp.height).max(0.0);
                // `peek`, not `get`: this effect writes those signals, and reading them would make it its own
                // dependency and re-run it for its own correction.
                if scroll_x.peek() > max_x {
                    scroll_x.set(max_x);
                }
                if scroll_y.peek() > max_y {
                    scroll_y.set(max_y);
                }
            })
        };

        // Registered on the CONTENT node, not the viewport leaf: the content is laid out as its own root (see the effect above), so the leaf is never its ancestor and a subtree test against it would miss.
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
        // Its own box rather than `LayoutLeaf::at_layout_position`: the content is positioned by the scroll
        // offset inside the viewport, not by the leaf's own placement, so the two must not both apply.
        let content = self.core.view(self.leaf.rect.get());
        if ui_tree::element_capture() {
            let semantics = renderer_core::Semantics::of(renderer_core::Role::ScrollArea);
            let element = crate::element::with_semantics(self.leaf.node, semantics);
            RenderNode::element(element, [content])
        } else {
            content
        }
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        // A scroll the surface already performed: the content is drawn where it now is, and what needs
        // correcting is this widget's idea of where that is.
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
mod tests {
    use crate::context::reset_layout_runtime;
    use geometry_core::Rect;
    use layout_core::{AvailableSpace, LayoutStyle, NodeId, SizeDimension};
    use platform_core::{Event, PointerSource, ScrollDelta};
    use renderer_core::DrawCommand;
    use ui_tree::{Component, EventResult, RenderNode};

    use super::*;
    use crate::canvas::Canvas;
    use crate::context::{compute_layout, new_container, track_layout};
    use crate::layout_item::LayoutItem;
    use crate::layout_leaf::LayoutLeaf;

    // Laying out only the scroll node must, via its effect, lay out the detached content subtree too —
    // a `scroll` element has no other owner to compute its content.
    #[test]
    fn scroll_area_lays_out_its_detached_content() {
        reset_layout_runtime();
        let content = Canvas::new(LayoutStyle::new().width(400.0).height(1000.0), |_| {
            RenderNode::Empty
        })
        .unwrap();
        let content_node = content.layout_node();
        let scroll = LayoutScrollArea::new(
            LayoutStyle::new().width(300.0).height(160.0),
            Box::new(content),
        )
        .unwrap();
        let scroll_node = scroll.layout_node();
        compute_layout(
            scroll_node,
            AvailableSpace::Definite(300.0),
            AvailableSpace::Definite(160.0),
        )
        .unwrap();
        let content_rect = track_layout(content_node).unwrap().get();
        assert!(
            content_rect.height > 0.0,
            "scroll must lay out its content, got {content_rect:?}"
        );
    }

    /// A page swapped for a shorter one must not leave the viewport looking at nothing.
    ///
    /// This is the bug as a user meets it: scroll down a long page, switch to a short one, and the panel is
    /// blank — the offset is still 600 while there is 200 of content, so the transform has pushed all of it
    /// out of the clip. It comes back on the next wheel tick, which is what makes it read as a repaint bug
    /// rather than a scroll one. Nothing here touches an input event: the layout alone has to put it right.
    #[test]
    fn content_that_gets_shorter_pulls_the_view_back_into_it() {
        reset_layout_runtime();
        let tall = Canvas::new(LayoutStyle::new().width(300.0).height(900.0), |_| {
            RenderNode::Empty
        })
        .unwrap();
        let short = Canvas::new(LayoutStyle::new().width(300.0).height(120.0), |_| {
            RenderNode::Empty
        })
        .unwrap();
        let short_node = short.layout_node();
        let page = crate::container::Container::new(
            LayoutStyle::new().flex_column(),
            vec![Box::new(tall) as Box<dyn LayoutItem>],
        )
        .unwrap();
        let page_node = page.layout_node();
        let scroll = LayoutScrollArea::new(
            LayoutStyle::new().width(300.0).height(200.0),
            Box::new(page),
        )
        .unwrap();
        compute_layout(
            scroll.layout_node(),
            AvailableSpace::Definite(300.0),
            AvailableSpace::Definite(200.0),
        )
        .unwrap();

        scroll.core.scroll_y.set(600.0);
        assert_eq!(scroll.core.scroll_y.get(), 600.0, "600 of 700 scrollable");

        // The page is swapped, exactly as a reactive list swaps what it shows.
        crate::context::set_children(page_node, &[short_node]).unwrap();
        crate::context::mark_dirty(page_node).unwrap();
        crate::context::relayout_if_dirty();

        assert_eq!(
            scroll.core.scroll_y.get(),
            0.0,
            "a page shorter than the viewport has nothing to scroll, so the view is at its top — not \
             600px past the end of it, drawing nothing"
        );
    }

    /// A viewport command called from an effect must not subscribe that effect to the offset it writes.
    ///
    /// This is how the fix for "the page changed, put the view back at the top" turned into "the page can no
    /// longer be scrolled at all": the effect noticing the page change also *read* the offset, so every wheel
    /// tick re-ran it, and it dutifully put the view back at the top. Both commands are `peek`-only for this
    /// reason, and both are exercised here — the launcher's follow-the-selection effect calls `reveal` from
    /// exactly the same place.
    #[test]
    fn a_viewport_command_run_from_an_effect_does_not_undo_the_users_own_scrolling() {
        reset_layout_runtime();
        let content = Canvas::new(LayoutStyle::new().width(300.0).height(900.0), |_| {
            RenderNode::Empty
        })
        .unwrap();
        let row = content.layout_node();
        let captured: Rc<RefCell<Option<ScrollViewport>>> = Rc::new(RefCell::new(None));
        let sink = Rc::clone(&captured);
        let scroll = LayoutScrollArea::new_with(
            LayoutStyle::new().width(300.0).height(200.0),
            move |viewport| {
                *sink.borrow_mut() = Some(viewport);
                Ok(Box::new(content) as Box<dyn LayoutItem>)
            },
        )
        .unwrap();
        compute_layout(
            scroll.layout_node(),
            AvailableSpace::Definite(300.0),
            AvailableSpace::Definite(200.0),
        )
        .unwrap();
        let viewport = captured.borrow().clone().expect("the builder ran");

        // The shape every caller uses: an effect over what it is following, issuing a command.
        let page = signal(0u32);
        let watched = page.read_only();
        let commanded = viewport.clone();
        let followed = watched;
        let _follow = effect(move || {
            followed.get();
            commanded.scroll_to_top();
        });

        scroll.core.scroll_y.set(150.0);
        reactive_core::batch(|| {});
        assert_eq!(
            scroll.core.scroll_y.get(),
            150.0,
            "scrolling is not a page change, so the effect must not have run and pulled the view back"
        );

        page.set(1);
        reactive_core::batch(|| {});
        assert_eq!(
            scroll.core.scroll_y.get(),
            0.0,
            "and the command still does its job when the thing it follows actually changes"
        );

        // `reveal` is the same shape and the same trap.
        let revealing = viewport.clone();
        let _follow_row = effect(move || {
            watched.get();
            revealing.reveal(row, 4.0);
        });
        scroll.core.scroll_y.set(80.0);
        reactive_core::batch(|| {});
        assert_eq!(
            scroll.core.scroll_y.get(),
            80.0,
            "a row already in view is left alone, and scrolling must not re-ask about it"
        );
    }

    /// The same rule from the other side: the content stayed, the window grew.
    #[test]
    fn a_taller_viewport_pulls_the_view_back_into_the_content() {
        reset_layout_runtime();
        let content = Canvas::new(LayoutStyle::new().width(300.0).height(500.0), |_| {
            RenderNode::Empty
        })
        .unwrap();
        // Inside a column that fills the surface, which is how a page area actually gets its height: the
        // viewport is whatever is left over, so resizing the surface resizes it.
        let scroll = LayoutScrollArea::new(
            LayoutStyle::new()
                .width(SizeDimension::Percent(1.0))
                .flex_grow(1.0)
                .min_height(0.0),
            Box::new(content),
        )
        .unwrap();
        let scroll_node = scroll.layout_node();
        let surface = new_container(
            LayoutStyle::new()
                .flex_column()
                .width(SizeDimension::Percent(1.0))
                .height(SizeDimension::Percent(1.0)),
            &[scroll_node],
        )
        .unwrap();
        compute_layout(
            surface,
            AvailableSpace::Definite(300.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();
        scroll.core.scroll_y.set(400.0);

        // The surface is resized taller — a float dragged by its grip, a monitor that changed mode.
        crate::context::mark_dirty(surface).unwrap();
        compute_layout(
            surface,
            AvailableSpace::Definite(300.0),
            AvailableSpace::Definite(450.0),
        )
        .unwrap();

        assert_eq!(
            scroll.core.scroll_y.get(),
            50.0,
            "500 of content in 450 of window leaves 50 to scroll, and that is where the view lands"
        );
    }

    /// A rebuilt tree — a shell following a config edit — is a second scroll area over the same offsets.
    #[test]
    fn a_scroll_built_on_kept_offsets_opens_where_the_last_one_left_off() {
        reset_layout_runtime();
        let offset = (signal(0.0f32), signal(0.0f32));
        let content = Canvas::new(LayoutStyle::new().width(300.0).height(900.0), |_| {
            RenderNode::Empty
        })
        .unwrap();
        let first = LayoutScrollArea::new_keeping(
            LayoutStyle::new().width(300.0).height(200.0),
            offset,
            |_| Ok(Box::new(content) as Box<dyn LayoutItem>),
        )
        .unwrap();
        compute_layout(
            first.layout_node(),
            AvailableSpace::Definite(300.0),
            AvailableSpace::Definite(200.0),
        )
        .unwrap();
        first.core.scroll_y.set(320.0);
        drop(first);

        let content = Canvas::new(LayoutStyle::new().width(300.0).height(900.0), |_| {
            RenderNode::Empty
        })
        .unwrap();
        let rebuilt = LayoutScrollArea::new_keeping(
            LayoutStyle::new().width(300.0).height(200.0),
            offset,
            |_| Ok(Box::new(content) as Box<dyn LayoutItem>),
        )
        .unwrap();
        assert_eq!(
            rebuilt.core.scroll_y.get(),
            320.0,
            "the tree was replaced, the reader's place in it was not"
        );
        compute_layout(
            rebuilt.layout_node(),
            AvailableSpace::Definite(300.0),
            AvailableSpace::Definite(200.0),
        )
        .unwrap();
        assert_eq!(
            rebuilt.core.scroll_y.get(),
            320.0,
            "and laying the rebuilt content out does not throw it away — a 0×0 rect is 'not measured yet', \
             not 'nothing to show'"
        );
    }

    // The end-to-end shape of the anchored-overlay bug: a trigger deep inside a real scroll area, scrolled away from where it was laid out. Exercises the two things the unit tests cannot: that the area registers its content (not its viewport leaf, which is never the content's ancestor), and that the subtree test reaches into the separately-computed content root.
    #[test]
    fn a_trigger_scrolled_inside_a_scroll_area_anchors_where_it_is_drawn() {
        reset_layout_runtime();
        let spacer = Canvas::new(LayoutStyle::new().width(400.0).height(600.0), |_| {
            RenderNode::Empty
        })
        .unwrap();
        let trigger = Canvas::new(LayoutStyle::new().width(80.0).height(24.0), |_| {
            RenderNode::Empty
        })
        .unwrap();
        let trigger_node = trigger.layout_node();
        let content = crate::container::Container::new(
            LayoutStyle::new().flex_column(),
            vec![Box::new(spacer) as Box<dyn LayoutItem>, Box::new(trigger)],
        )
        .unwrap();
        let scroll = LayoutScrollArea::new(
            LayoutStyle::new().width(300.0).height(200.0),
            Box::new(content),
        )
        .unwrap();
        compute_layout(
            scroll.layout_node(),
            AvailableSpace::Definite(300.0),
            AvailableSpace::Definite(200.0),
        )
        .unwrap();

        let laid_out = crate::context::absolute_rect(trigger_node).unwrap();
        assert!(laid_out.y > 200.0, "the trigger starts below the fold");
        assert_eq!(
            crate::scroll_region::visible_rect(trigger_node),
            Some(laid_out),
            "unscrolled, drawn position and laid-out position agree"
        );

        scroll.core.scroll_y.set(150.0);
        let drawn = crate::scroll_region::visible_rect(trigger_node).unwrap();
        assert_eq!(
            drawn.y,
            laid_out.y - 150.0,
            "scrolling down draws the trigger higher, and that is where a panel must anchor"
        );

        // Dropping the area withdraws its registration, so a later query is not shifted by a dead viewport.
        drop(scroll);
        assert_eq!(
            crate::scroll_region::visible_rect(trigger_node),
            Some(laid_out)
        );
    }

    pub(crate) fn make_scroll_area() -> ScrollArea {
        reset_layout_runtime();
        let content = Canvas::new(LayoutStyle::new().width(400.0).height(1000.0), |_| {
            RenderNode::Empty
        })
        .unwrap();
        let node = content.layout_node();
        let sa = ScrollArea::new(|| Rect::new(0.0, 0.0, 400.0, 300.0), Box::new(content));
        compute_layout(
            node,
            AvailableSpace::Definite(400.0),
            AvailableSpace::MaxContent,
        )
        .unwrap();
        sa
    }

    /// A wheel turn in the middle of the 400×300 viewport every fixture here uses.
    fn settle() {
        let start = std::time::Instant::now();
        for frame in 1..=40 {
            motion_core::tick(start + std::time::Duration::from_millis(16 * frame));
        }
    }

    fn wheel_over(delta: ScrollDelta) -> Event {
        Event::Scrolled {
            delta,
            x: 100.0,
            y: 100.0,
        }
    }

    fn make_scroll_area_small() -> ScrollArea {
        reset_layout_runtime();
        let content = Canvas::new(LayoutStyle::new().width(400.0).height(200.0), |_| {
            RenderNode::Empty
        })
        .unwrap();
        let node = content.layout_node();
        let sa = ScrollArea::new(|| Rect::new(0.0, 0.0, 400.0, 300.0), Box::new(content));
        compute_layout(
            node,
            AvailableSpace::Definite(400.0),
            AvailableSpace::MaxContent,
        )
        .unwrap();
        sa
    }

    #[test]
    fn a_click_inside_scrolled_content_that_rerenders_it_does_not_panic() {
        use crate::container::Container;
        use crate::context::track_layout;
        use crate::styled_container::StyledContainer;
        use platform_core::PointerButton;
        use reactive_core::{begin_batch, end_batch, signal};

        reset_layout_runtime();
        let s = signal(0i32);
        let s_cb = s;
        // A pressable primitive stands in for the old high-level Button (now in ui-components).
        let btn = StyledContainer::new(
            LayoutStyle::new().width(50.0).height(30.0),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .on_press(move || s_cb.update(|n| *n += 1));
        let btn_node = btn.layout_node();
        let s_txt = s;
        let txt = crate::text::Text::new(
            move || format!("{}", s_txt.get()),
            LayoutStyle::new().width(50.0).height(20.0),
            || renderer_core::TextStyle::new(14.0, renderer_core::Color::BLACK),
        )
        .unwrap();
        let content = Container::new(
            LayoutStyle::new().flex_column().width(400.0).height(1000.0),
            vec![Box::new(btn), Box::new(txt)],
        )
        .unwrap();
        let content_node = content.layout_node();
        let sa = ScrollArea::new(|| Rect::new(0.0, 0.0, 400.0, 300.0), Box::new(content));
        compute_layout(
            content_node,
            AvailableSpace::Definite(400.0),
            AvailableSpace::MaxContent,
        )
        .unwrap();
        let br = track_layout(btn_node).unwrap().get();

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
        use crate::container::Container;
        use crate::context::track_layout;
        use crate::styled_container::StyledContainer;
        use platform_core::PointerButton;
        use reactive_core::signal;

        reset_layout_runtime();
        let s = signal(0i32);
        let s_cb = s;
        // A pressable primitive stands in for the old high-level Button (now in ui-components).
        let btn = StyledContainer::new(
            LayoutStyle::new().width(50.0).height(30.0),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .on_press(move || s_cb.update(|n| *n += 1));
        let btn_node = btn.layout_node();
        let content = Container::new(
            LayoutStyle::new().flex_column().width(400.0).height(1000.0),
            vec![Box::new(btn)],
        )
        .unwrap();
        let content_node = content.layout_node();
        let mut sa = ScrollArea::new(|| Rect::new(0.0, 0.0, 400.0, 300.0), Box::new(content));
        compute_layout(
            content_node,
            AvailableSpace::Definite(400.0),
            AvailableSpace::MaxContent,
        )
        .unwrap();
        let br = track_layout(btn_node).unwrap().get();
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
                x: cx,
                y: cy,
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
        reset_layout_runtime();
        let content = Canvas::new(LayoutStyle::new().width(400.0).height(1000.0), |_| {
            RenderNode::Empty
        })
        .unwrap();
        let content_node = content.layout_node();
        let sa = LayoutScrollArea::new(
            LayoutStyle::new().width(400.0).height(300.0),
            Box::new(content),
        )
        .unwrap();
        let root = new_container(
            LayoutStyle::new().flex_column().width(400.0).height(300.0),
            &[sa.layout_node()],
        )
        .unwrap();
        compute_layout(
            root,
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(300.0),
        )
        .unwrap();
        compute_layout(
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
        reset_layout_runtime();
        let content = Canvas::new(LayoutStyle::new().width(400.0).height(1000.0), |_| {
            RenderNode::Empty
        })
        .unwrap();
        let content_node = content.layout_node();
        let sa = LayoutScrollArea::new(
            LayoutStyle::new().width(400.0).height(300.0),
            Box::new(content),
        )
        .unwrap();
        let root = new_container(
            LayoutStyle::new().flex_column().width(400.0).height(300.0),
            &[sa.layout_node()],
        )
        .unwrap();
        compute_layout(
            root,
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(300.0),
        )
        .unwrap();
        compute_layout(
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
        sa.on_event(&wheel_over(ScrollDelta::Lines { x: 0.0, y: -3.0 }));
        settle();
        assert_eq!(sa.core.scroll_y.get(), 60.0);
    }

    // Nested scroll: a wheel event is ignored when it happened outside this viewport (it belongs to an
    // ancestor, or to an inner scroll that already consumed it), so the outer area does not steal it.
    #[test]
    fn wheel_outside_viewport_does_not_scroll() {
        let mut sa = make_scroll_area(); // viewport 400x300
        let result = sa.on_event(&Event::Scrolled {
            delta: ScrollDelta::Lines { x: 0.0, y: -3.0 },
            x: 500.0,
            y: 500.0,
        });
        assert_eq!(result, EventResult::Ignored);
        assert_eq!(
            sa.core.scroll_y.get(),
            0.0,
            "must not scroll when the wheel turned elsewhere"
        );
    }

    /// The wheel carries where it happened, so the first one lands correctly even though the pointer has
    /// never moved — the case a viewport that tracked moves itself could only guess at.
    #[test]
    fn wheel_inside_viewport_scrolls_without_a_prior_move() {
        let mut sa = make_scroll_area();
        sa.on_event(&wheel_over(ScrollDelta::Lines { x: 0.0, y: -3.0 }));
        settle();
        assert!(
            sa.core.scroll_y.get() > 0.0,
            "wheel over the viewport scrolls it"
        );
    }

    /// What a terminal does: its smallest visible step is a whole cell, so the notch is applied at once.
    #[test]
    fn a_surface_that_cannot_show_the_gap_takes_the_notch_whole() {
        let was = ui_tree::set_smooth_wheel(false);
        let mut sa = make_scroll_area();
        sa.on_event(&wheel_over(ScrollDelta::Lines { x: 0.0, y: -3.0 }));
        assert_eq!(sa.core.scroll_y.get(), 60.0, "no frame clock involved");
        ui_tree::set_smooth_wheel(was);
    }

    /// A finger landing on a laptop that has both a wheel and a touch screen takes the offset over.
    #[test]
    fn a_press_catches_a_notch_still_being_covered() {
        let mut sa = make_scroll_area();
        sa.on_event(&wheel_over(ScrollDelta::Lines { x: 0.0, y: -3.0 }));
        let start = std::time::Instant::now();
        motion_core::tick(start);
        motion_core::tick(start + std::time::Duration::from_millis(16));
        let caught = sa.core.scroll_y.get();
        assert!((0.0..60.0).contains(&caught), "partway there: {caught}");
        sa.on_event(&Event::PointerPressed {
            x: 10.0,
            y: 10.0,
            button: platform_core::PointerButton::Primary,
            source: platform_core::PointerSource::Mouse,
        });
        settle();
        assert_eq!(sa.core.scroll_y.get(), caught, "the press is in charge now");
    }

    #[test]
    fn scroll_pixels_updates_offset() {
        let mut sa = make_scroll_area();
        sa.on_event(&wheel_over(ScrollDelta::Pixels { x: 0.0, y: -80.0 }));
        assert_eq!(sa.core.scroll_y.get(), 80.0);
    }

    #[test]
    fn scroll_clamps_to_max() {
        let mut sa = make_scroll_area();
        sa.on_event(&wheel_over(ScrollDelta::Pixels { x: 0.0, y: -9999.0 }));
        assert_eq!(sa.core.scroll_y.get(), 700.0);
    }

    #[test]
    fn scroll_clamps_to_zero() {
        let mut sa = make_scroll_area();
        sa.on_event(&wheel_over(ScrollDelta::Pixels { x: 0.0, y: 9999.0 }));
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

        reset_layout_runtime();
        let leaf = LayoutLeaf::register(LayoutStyle::new().width(400.0).height(1000.0)).unwrap();
        let node = leaf.node;
        let content = CapturingItem {
            leaf,
            out: captured_y_clone,
        };
        let mut sa = ScrollArea::new(|| Rect::new(100.0, 50.0, 400.0, 300.0), Box::new(content));
        compute_layout(
            node,
            AvailableSpace::Definite(400.0),
            AvailableSpace::MaxContent,
        )
        .unwrap();

        sa.on_event(&Event::Scrolled {
            delta: ScrollDelta::Pixels { x: 0.0, y: -100.0 },
            x: 150.0,
            y: 200.0,
        });

        sa.on_event(&Event::PointerMoved {
            x: 150.0,
            y: 200.0,
            source: PointerSource::Mouse,
        });

        assert!((captured_y.get() - 250.0).abs() < 0.001);
    }

    pub(crate) fn make_scroll_area_wide() -> ScrollArea {
        reset_layout_runtime();
        let content = Canvas::new(LayoutStyle::new().width(1000.0).height(300.0), |_| {
            RenderNode::Empty
        })
        .unwrap();
        let node = content.layout_node();
        let sa = ScrollArea::new(|| Rect::new(0.0, 0.0, 400.0, 300.0), Box::new(content));
        compute_layout(
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
        sa.on_event(&wheel_over(ScrollDelta::Lines { x: -3.0, y: 0.0 }));
        settle();
        assert_eq!(sa.core.scroll_x.get(), 60.0);
    }

    #[test]
    fn scroll_x_pixels_updates_offset() {
        let mut sa = make_scroll_area_wide();
        sa.on_event(&wheel_over(ScrollDelta::Pixels { x: -80.0, y: 0.0 }));
        assert_eq!(sa.core.scroll_x.get(), 80.0);
    }

    #[test]
    fn scroll_x_clamps_to_max() {
        let mut sa = make_scroll_area_wide();
        sa.on_event(&wheel_over(ScrollDelta::Pixels { x: -9999.0, y: 0.0 }));
        assert_eq!(sa.core.scroll_x.get(), 600.0);
    }

    #[test]
    fn scroll_x_clamps_to_zero() {
        let mut sa = make_scroll_area_wide();
        sa.on_event(&wheel_over(ScrollDelta::Pixels { x: 9999.0, y: 0.0 }));
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

#[cfg(test)]
mod scrollbar_tests {
    use super::tests::{make_scroll_area, make_scroll_area_wide};
    use super::*;
    use crate::canvas::Canvas;
    use crate::context::{compute_layout, reset_layout_runtime};
    use layout_core::AvailableSpace;
    use platform_core::{PointerButton, PointerSource};

    fn press(x: f32, y: f32) -> Event {
        Event::PointerPressed {
            x: x as f64,
            y: y as f64,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        }
    }

    fn moved(x: f32, y: f32) -> Event {
        Event::PointerMoved {
            x: x as f64,
            y: y as f64,
            source: PointerSource::Mouse,
        }
    }

    fn released(x: f32, y: f32) -> Event {
        Event::PointerReleased {
            x: x as f64,
            y: y as f64,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        }
    }

    // Viewport 400x300 over content 400x1000: a 90px thumb with 210px of travel over 700px of scroll.
    fn tall() -> ScrollArea {
        make_scroll_area()
    }

    /// The content moves by its own range, not by the distance the thumb was dragged.
    #[test]
    fn dragging_the_thumb_moves_the_content_in_proportion() {
        let mut sa = tall();
        assert_eq!(sa.on_event(&press(396.0, 45.0)), EventResult::Handled);
        sa.on_event(&moved(396.0, 150.0));
        assert_eq!(
            sa.core.scroll_y.get(),
            350.0,
            "half the travel, half the range"
        );
        sa.on_event(&moved(396.0, 400.0));
        assert_eq!(sa.core.scroll_y.get(), 700.0, "and it stops at the end");
    }

    #[test]
    fn the_thumb_is_grabbed_where_it_was_taken_hold_of() {
        let mut sa = tall();
        // Held near its lower edge: the thumb must not jump so its top lands under the pointer.
        sa.on_event(&press(396.0, 80.0));
        sa.on_event(&moved(396.0, 80.0));
        assert_eq!(sa.core.scroll_y.get(), 0.0, "no jump on the first move");
    }

    #[test]
    fn a_press_on_the_track_pages_towards_it() {
        let mut sa = tall();
        sa.on_event(&press(396.0, 200.0));
        assert_eq!(sa.core.scroll_y.get(), 300.0, "one viewport down");
        sa.on_event(&released(396.0, 200.0));
        sa.on_event(&press(396.0, 5.0));
        assert_eq!(sa.core.scroll_y.get(), 0.0, "and back up again");
    }

    #[test]
    fn letting_go_ends_the_drag() {
        let mut sa = tall();
        sa.on_event(&press(396.0, 45.0));
        sa.on_event(&moved(396.0, 150.0));
        sa.on_event(&released(396.0, 150.0));
        let settled = sa.core.scroll_y.get();
        sa.on_event(&moved(396.0, 250.0));
        assert_eq!(sa.core.scroll_y.get(), settled, "the pointer is free again");
    }

    /// The bar is a strip at the edge; a press anywhere else is the content's.
    #[test]
    fn a_press_beside_the_bar_is_not_a_grab() {
        let mut sa = tall();
        sa.on_event(&press(100.0, 45.0));
        sa.on_event(&moved(100.0, 250.0));
        assert_eq!(sa.core.scroll_y.get(), 0.0);
    }

    #[test]
    fn there_is_nothing_to_grab_when_the_content_fits() {
        reset_layout_runtime();
        let content = Canvas::new(LayoutStyle::new().width(400.0).height(120.0), |_| {
            RenderNode::Empty
        })
        .unwrap();
        let node = content.layout_node();
        let mut sa = ScrollArea::new(|| Rect::new(0.0, 0.0, 400.0, 300.0), Box::new(content));
        compute_layout(
            node,
            AvailableSpace::Definite(400.0),
            AvailableSpace::MaxContent,
        )
        .unwrap();
        assert_ne!(
            sa.on_event(&press(396.0, 45.0)),
            EventResult::Handled,
            "no bar is drawn, so nothing there answers a press"
        );
    }

    /// Viewport 400x300 over content 1000x300: a 160px thumb with 240px of travel over 600px of scroll.
    #[test]
    fn the_horizontal_bar_drags_the_same_way() {
        let mut sa = make_scroll_area_wide();
        assert_eq!(sa.on_event(&press(80.0, 296.0)), EventResult::Handled);
        sa.on_event(&moved(200.0, 296.0));
        assert_eq!(sa.core.scroll_x.get(), 300.0);
    }
}
