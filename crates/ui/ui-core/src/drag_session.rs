//! [`DragSession`]: a payload of the app's own type carried by the pointer from a box in one widget to a place in another, inside the app.
//!
//! Not the operating system's drag and drop ([`drag_source`](crate::StyledContainer::drag_source), [`drop_target`](crate::StyledContainer::drop_target)), which carries data out of the app or into it. A session never leaves the surface: what is carried is a Rust value, and every box that takes part — the [items](DragSession::item) it is picked up from and the [areas](DragSession::area) it can be dropped on — shares the one session, whichever widget built it.

use std::cell::{Cell, RefCell};
use std::cmp::Reverse;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use geometry_core::Rect;
use layout_core::NodeId;
use platform_core::{Cursor, PointerButton};
use reactive_core::{
    OwnerId, Reactive, RwSignal, Timer, dispose_owner, effect, on_cleanup, root_scope, run_after,
    signal, timer_now, untrack,
};
use web_time::Instant;

use crate::context::track_layout;
use crate::drag::{Held, HeldStroke, drag_start, hold_stroke};
use crate::input_region::{from_surface, placed_in, placement_depth, to_surface, visible_rect};
use crate::layout_item::LayoutItem;
use crate::reorder::{Axis, insertion_index};
use crate::scroll_area::ScrollViewport;
use crate::scroll_viewports::scroll_viewports_of;
use crate::styled_container::StyledContainer;

/// How long the pointer rests on one item of an area before the area hears it was [held there](DropArea::on_hold).
const HOLD_DELAY: Duration = Duration::from_millis(700);

/// How far in from a scroll area's edge the pointer starts scrolling it, or a quarter of the area where that is less.
const SCROLL_EDGE: f32 = 40.0;

/// How fast a scroll area scrolls with the pointer at its very edge, in logical px a second; it slows to nothing at the inner side of the band.
const SCROLL_SPEED: f32 = 1200.0;

/// How often a scroll area held at its edge moves on.
const SCROLL_TICK: Duration = Duration::from_millis(16);

/// The most time one scroll step makes up for, so a loop that stalled does not jump the content by what the stall would have scrolled.
const MAX_SCROLL_STEP: Duration = Duration::from_millis(100);

/// How far the pointer travels from a press on an item before the press is a drag rather than a click.
const DRAG_THRESHOLD: f32 = 4.0;

/// Where in a [`DropArea`] a drop lands: between two of its items, or onto one of them.
///
/// `At` is how the place counts the items: a position in display order in an area, or whatever a widget that holds several lists counts them by. `Between(n)` is the gap before the `n`th item, so `Between(len)` is the one after the last.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DropPlace<At> {
    Between(At),
    Inside(At),
}

impl<At> DropPlace<At> {
    /// The item the place is counted from.
    pub fn at(&self) -> &At {
        match self {
            Self::Between(at) | Self::Inside(at) => at,
        }
    }

    /// The same place, counted another way.
    pub fn map<B>(self, f: impl FnOnce(At) -> B) -> DropPlace<B> {
        match self {
            Self::Between(at) => DropPlace::Between(f(at)),
            Self::Inside(at) => DropPlace::Inside(f(at)),
        }
    }
}

/// Names one [`DropArea`] of a session, for as long as it is registered.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DropAreaId(u64);

/// Where a [`DropArea`] and its items are, in the space its node's children are laid out in: the content's own coordinates for an area inside a scroll area, which do not change as it scrolls.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AreaLayout {
    /// Where the pointer is over the area at all.
    pub bounds: Rect,
    /// The items a drop lands between or onto, in display order along the area's axis.
    pub items: Vec<Rect>,
}

/// A box the pointer can pick a payload up from: see [`DragSession::item`].
pub struct DragItem<P> {
    payload: Rc<dyn Fn() -> Option<P>>,
    label: Option<Rc<dyn Fn() -> String>>,
    threshold: f32,
    on_detach: Option<Rc<dyn Fn(P, (f32, f32))>>,
    along: Option<Axis>,
    within: Option<Rc<dyn Fn() -> Rect>>,
}

impl<P> DragItem<P> {
    /// An item that carries what `payload` answers as the drag starts; `None` leaves the stroke undragged.
    pub fn new(payload: impl Fn() -> Option<P> + 'static) -> Self {
        Self {
            payload: Rc::new(payload),
            label: None,
            threshold: DRAG_THRESHOLD,
            on_detach: None,
            along: None,
            within: None,
        }
    }

    /// What the carried payload is called, read as the drag starts and shown beside the pointer by whoever draws the session's [`label`](DragSession::label).
    pub fn label(self, label: impl Fn() -> String + 'static) -> Self {
        Self {
            label: Some(Rc::new(label)),
            ..self
        }
    }

    /// How far the pointer travels from the press before it is a drag rather than a click on the box. 4 px unless set.
    pub fn threshold(self, px: f32) -> Self {
        Self {
            threshold: px.max(0.0),
            ..self
        }
    }

    /// Told the payload and where on the surface it was let go, when it is let go over no area that accepts it.
    pub fn on_detach(self, on_detach: impl Fn(P, (f32, f32)) + 'static) -> Self {
        Self {
            on_detach: Some(Rc::new(on_detach)),
            ..self
        }
    }

    /// Carries the payload along `axis` only: across it, the pointer counts as staying where it picked the payload up.
    pub fn along(self, axis: Axis) -> Self {
        Self {
            along: Some(axis),
            ..self
        }
    }

    /// Carries the payload inside `bounds` only, in the space the item is laid out in, read at each move: past them, the pointer counts as standing at their edge.
    pub fn within(self, bounds: impl Fn() -> Rect + 'static) -> Self {
        Self {
            within: Some(Rc::new(bounds)),
            ..self
        }
    }
}

/// A place a carried payload can be dropped: see [`DragSession::area`].
pub struct DropArea<P> {
    node: NodeId,
    axis: Axis,
    layout: Rc<dyn Fn() -> AreaLayout>,
    accepts: Rc<dyn Fn(&P, DropPlace<usize>) -> bool>,
    on_drop: Rc<dyn Fn(P, DropPlace<usize>)>,
    on_hold: Option<Rc<dyn Fn(&P, usize)>>,
}

impl<P> DropArea<P> {
    /// The box `node`, whose items run along `axis`. `accepts` answers whether a payload may land at a place, and `on_drop` is told one that did.
    ///
    /// Until [`layout`](Self::layout) says otherwise the area is the box's own rect with no items, so the only place in it is `Between(0)`.
    pub fn new(
        node: NodeId,
        axis: Axis,
        accepts: impl Fn(&P, DropPlace<usize>) -> bool + 'static,
        on_drop: impl Fn(P, DropPlace<usize>) + 'static,
    ) -> Self {
        Self {
            node,
            axis,
            layout: Rc::new(move || AreaLayout {
                bounds: track_layout(node)
                    .map(|rect| rect.peek())
                    .unwrap_or_default(),
                items: Vec::new(),
            }),
            accepts: Rc::new(accepts),
            on_drop: Rc::new(on_drop),
            on_hold: None,
        }
    }

    /// Where the area and its items are, read each time the pointer is measured against them, and again whenever a signal it reads changes while a payload is carried.
    pub fn layout(self, layout: impl Fn() -> AreaLayout + 'static) -> Self {
        Self {
            layout: Rc::new(layout),
            ..self
        }
    }

    /// Told which item the pointer has rested on while carrying a payload, once it has rested there for the session's [hold delay](DragSession::hold_delay): what opens a folder held over.
    pub fn on_hold(self, on_hold: impl Fn(&P, usize) + 'static) -> Self {
        Self {
            on_hold: Some(Rc::new(on_hold)),
            ..self
        }
    }
}

/// A drag in progress, as the widgets that draw it read it.
#[derive(Clone)]
struct Stroke<P> {
    payload: P,
    label: Option<Arc<str>>,
    /// The item it was picked up from.
    from: NodeId,
    /// The box the item is laid out in, which the points below are measured in: kept apart from the item, which a list that builds only the rows on screen may drop mid-stroke.
    space: Option<NodeId>,
    /// Where it was picked up, in `space`.
    grabbed: (f32, f32),
    /// Where the pointer is, on the surface.
    at: (f32, f32),
    over: Option<(DropAreaId, DropPlace<usize>)>,
}

/// What a session holds while a stroke runs, beyond what the widgets read: the pointer it follows, what watches the areas move, and the item's own say over where the payload goes.
struct Hand<P> {
    _held: HeldStroke,
    watch: OwnerId,
    item: Rc<DragItem<P>>,
}

impl<P> Drop for Hand<P> {
    fn drop(&mut self) {
        dispose_owner(self.watch);
    }
}

/// What the pointer at one place is over.
struct Reading {
    over: Option<(DropAreaId, DropPlace<usize>)>,
    resting: Option<(DropAreaId, usize)>,
}

struct Session<P: Clone + 'static> {
    stroke: RwSignal<Option<Stroke<P>>>,
    hand: RefCell<Option<Hand<P>>>,
    areas: RefCell<Vec<(DropAreaId, Rc<DropArea<P>>)>>,
    next_area: Cell<u64>,
    hold_delay: Cell<Duration>,
    resting: Cell<Option<(DropAreaId, usize)>>,
    hold: RefCell<Option<Timer>>,
    scroll: RefCell<Option<Timer>>,
    scrolled_at: Cell<Instant>,
}

/// Carries a payload of the app's type `P` from an [item](Self::item) to an [area](Self::area), wherever in the app either was built.
///
/// Once a press on an item travels past its threshold the session holds the stroke itself, so the item may go away mid-stroke — a row a list stops building once it scrolls out of view — and the payload is still carried to where it is let go. While the pointer carries it, the session works out which area and which place in it the payload would land at, asking the area whether it [accepts](DropArea::new) the payload there, and measures again whenever an area is laid out anew; scrolls a scroll area the pointer holds near its edge; and tells an area which of its items the pointer [rests on](DropArea::on_hold). Letting go drops it on the area, or tells the item it was [let go](DragItem::on_detach) over none. Escape, another button or the window losing focus puts everything back as it was before the press.
///
/// The session draws nothing. Each widget reads where the payload would land ([`place_in`](Self::place_in)) to draw its own line or highlight, and whoever shows the [`label`](Self::label) beside the [`pointer`](Self::pointer) draws that.
///
/// The keyboard does not go through a session: a widget moves its items with keys of its own and reports the move the way a drop is reported.
pub struct DragSession<P: Clone + 'static>(Rc<Session<P>>);

impl<P: Clone + 'static> Clone for DragSession<P> {
    fn clone(&self) -> Self {
        Self(Rc::clone(&self.0))
    }
}

impl<P: Clone + 'static> Default for DragSession<P> {
    fn default() -> Self {
        Self::new()
    }
}

impl<P: Clone + 'static> DragSession<P> {
    pub fn new() -> Self {
        Self(Rc::new(Session {
            stroke: signal(None),
            hand: RefCell::new(None),
            areas: RefCell::new(Vec::new()),
            next_area: Cell::new(0),
            hold_delay: Cell::new(HOLD_DELAY),
            resting: Cell::new(None),
            hold: RefCell::new(None),
            scroll: RefCell::new(None),
            scrolled_at: Cell::new(timer_now()),
        }))
    }

    /// How long the pointer rests on an item before its area hears of it. 700 ms unless set.
    pub fn hold_delay(self, delay: Duration) -> Self {
        self.0.hold_delay.set(delay);
        self
    }

    /// Makes `container` an item the pointer picks `item`'s payload up from: a press that travels past the item's threshold starts carrying it, and the session carries it from there until it is let go, whether or not the box is still there. Shows the grab cursor over the box, and the grabbing one while it is the item carried.
    ///
    /// Sets the box's drag handlers and threshold, so neither is set on it elsewhere.
    pub fn item(&self, container: StyledContainer, item: DragItem<P>) -> StyledContainer {
        let node = container.layout_node();
        let item = Rc::new(item);
        let stroke = self.0.stroke;
        // One pick-up per press: a stroke cancelled with the button still down goes on reporting moves through the box, and must not pick the payload up again.
        let picked = Rc::new(Cell::new(false));
        container
            .cursor(Reactive::of(move || {
                match stroke.with(|stroke| stroke.as_ref().is_some_and(|s| s.from == node)) {
                    true => Cursor::Grabbing,
                    false => Cursor::Grab,
                }
            }))
            .drag_threshold(item.threshold)
            .on_drag({
                let session = Rc::clone(&self.0);
                let picked = Rc::clone(&picked);
                move |x, y| {
                    if !picked.replace(true) {
                        session.pick_up(node, &item, (x, y));
                    }
                }
            })
            .on_drag_end(move |_, _| picked.set(false))
    }

    /// Registers `area` as a place a carried payload can be dropped, until the owner registering it is disposed.
    pub fn area(&self, area: DropArea<P>) -> DropAreaId {
        let id = DropAreaId(self.0.next_area.get());
        self.0.next_area.set(id.0 + 1);
        self.0.areas.borrow_mut().push((id, Rc::new(area)));
        let session = Rc::downgrade(&self.0);
        on_cleanup(move || {
            if let Some(session) = session.upgrade() {
                session.areas.borrow_mut().retain(|(at, _)| *at != id);
            }
        });
        id
    }

    /// What the pointer carries, reactively: `None` with nothing in hand.
    pub fn carried(&self) -> Option<P> {
        self.0
            .stroke
            .with(|stroke| stroke.as_ref().map(|s| s.payload.clone()))
    }

    /// The item what the pointer carries was picked up from, reactively. It names the box as it was when the drag started, which a list that builds only the rows on screen may since have dropped.
    pub fn carried_from(&self) -> Option<NodeId> {
        self.0.stroke.with(|stroke| stroke.as_ref().map(|s| s.from))
    }

    /// What the carried payload is called, as its item's [`label`](DragItem::label) said when it was picked up, reactively.
    pub fn label(&self) -> Option<Arc<str>> {
        self.0
            .stroke
            .with(|stroke| stroke.as_ref().and_then(|s| s.label.clone()))
    }

    /// Where the pointer carrying a payload is, on the surface, reactively.
    pub fn pointer(&self) -> Option<(f32, f32)> {
        self.0.stroke.with(|stroke| stroke.as_ref().map(|s| s.at))
    }

    /// The area and the place in it the payload would land at if it were let go now, reactively: `None` over no area that accepts it.
    pub fn over(&self) -> Option<(DropAreaId, DropPlace<usize>)> {
        self.0
            .stroke
            .with(|stroke| stroke.as_ref().and_then(|s| s.over))
    }

    /// [`over`](Self::over) for one area: where in `area` the payload would land, or `None` while it would land anywhere else.
    pub fn place_in(&self, area: DropAreaId) -> Option<DropPlace<usize>> {
        self.over()
            .filter(|(over, _)| *over == area)
            .map(|(_, place)| place)
    }

    /// How far the pointer has carried the payload from where it was picked up, in the space its item is laid out in, reactively: what moves the item along with the pointer. It counts the content scrolled under a pointer held still, so an item in a scroll area stays under the pointer as the area scrolls.
    pub fn travel(&self) -> Option<(f32, f32)> {
        self.0.stroke.with(|stroke| {
            let stroke = stroke.as_ref()?;
            let here = in_space(stroke.space, stroke.at, false)?;
            Some((here.0 - stroke.grabbed.0, here.1 - stroke.grabbed.1))
        })
    }
}

impl<P: Clone + 'static> Session<P> {
    /// Picks up `item`'s payload from the box `from`, the item having reported the pointer at `local` to it, and holds the stroke from here on.
    fn pick_up(self: &Rc<Self>, from: NodeId, item: &Rc<DragItem<P>>, local: (f32, f32)) {
        if self.stroke.peek().is_some() {
            return;
        }
        let Some((grabbed, at)) = surface_point(from, local) else {
            return;
        };
        let Some(payload) = (item.payload)() else {
            return;
        };
        self.settle();
        let button = drag_start().map_or(PointerButton::Primary, |start| start.button);
        let session = Rc::downgrade(self);
        let held = hold_stroke(button, move |heard| {
            if let Some(session) = session.upgrade() {
                session.hear(heard);
            }
        });
        let hand = Hand {
            _held: held,
            watch: self.watch_areas(),
            item: Rc::clone(item),
        };
        *self.hand.borrow_mut() = Some(hand);
        self.update(Stroke {
            payload,
            label: item.label.as_ref().map(|label| Arc::from(label())),
            from,
            space: placed_in(from),
            grabbed,
            at,
            over: None,
        });
    }

    fn hear(self: &Rc<Self>, heard: Held) {
        match heard {
            Held::Moved(at) => {
                if let Some(stroke) = self.stroke.peek() {
                    let at = self.confined(&stroke, at);
                    self.update(Stroke { at, ..stroke });
                }
            }
            Held::Released(at) => self.land(at),
            Held::Cancelled => self.end(),
        }
    }

    /// Ends the stroke where the pointer let go at `at`, and tells the area it lands on, or the item when it lands on none.
    fn land(self: &Rc<Self>, at: (f32, f32)) {
        let Some(stroke) = self.stroke.peek() else {
            return;
        };
        let at = self.confined(&stroke, at);
        let reading = self.read(&stroke.payload, at);
        self.settle();
        let on_detach = self
            .hand
            .borrow()
            .as_ref()
            .and_then(|hand| hand.item.on_detach.clone());
        // Told while the stroke still reads as carried, so a widget can tell its own item from another's.
        match reading.over {
            Some((area, place)) => {
                if let Some(area) = self.area_of(area) {
                    (area.on_drop)(stroke.payload.clone(), place);
                }
            }
            None => {
                if let Some(on_detach) = on_detach {
                    on_detach(stroke.payload.clone(), at);
                }
            }
        }
        self.end();
    }

    /// Lets the stroke go: nothing is carried, and nothing waits on the pointer.
    fn end(&self) {
        self.settle();
        self.stroke.set(None);
        let hand = self.hand.borrow_mut().take();
        drop(hand);
    }

    /// Where the item lets the pointer at `at` carry the payload: along its axis and inside its bounds, when it keeps it to either.
    fn confined(&self, stroke: &Stroke<P>, at: (f32, f32)) -> (f32, f32) {
        let hand = self.hand.borrow();
        let Some(item) = hand.as_ref().map(|hand| &hand.item) else {
            return at;
        };
        if item.along.is_none() && item.within.is_none() {
            return at;
        }
        let Some(point) = in_space(stroke.space, at, false) else {
            return at;
        };
        let point = match item.along {
            Some(Axis::Horizontal) => (point.0, stroke.grabbed.1),
            Some(Axis::Vertical) => (stroke.grabbed.0, point.1),
            None => point,
        };
        let point = match &item.within {
            Some(bounds) => {
                let bounds = bounds();
                (
                    point.0.clamp(bounds.x, bounds.x + bounds.width),
                    point.1.clamp(bounds.y, bounds.y + bounds.height),
                )
            }
            None => point,
        };
        match stroke.space {
            Some(space) => to_surface(space, point),
            None => point,
        }
    }

    /// Measures `stroke` against the areas, writes it, and starts or stops what waits on where the pointer is.
    fn update(self: &Rc<Self>, stroke: Stroke<P>) {
        let reading = self.read(&stroke.payload, stroke.at);
        self.rest(reading.resting);
        let at = stroke.at;
        self.stroke.set(Some(Stroke {
            over: reading.over,
            ..stroke
        }));
        self.autoscroll(at);
    }

    /// Measures the stroke again where the pointer already is: for when what is under it moved and the pointer did not.
    fn remeasure(self: &Rc<Self>) {
        if let Some(stroke) = self.stroke.peek() {
            self.update(stroke);
        }
    }

    /// Watches the areas for as long as the owner it answers lives: each one's box and whatever its layout reads, so a stroke is measured again once the areas move under a pointer that does not, as a folder held open does to the rows below it.
    fn watch_areas(self: &Rc<Self>) -> OwnerId {
        let scope = root_scope();
        let session = Rc::downgrade(self);
        let seen: RefCell<Option<Vec<(Rect, AreaLayout)>>> = RefCell::new(None);
        effect(move || {
            let Some(session) = session.upgrade() else {
                return;
            };
            let areas = session.areas.borrow().clone();
            let now: Vec<(Rect, AreaLayout)> = areas
                .iter()
                .map(|(_, area)| {
                    let laid = track_layout(area.node)
                        .map(|rect| rect.get())
                        .unwrap_or_default();
                    (laid, (area.layout)())
                })
                .collect();
            let moved = seen.borrow().as_ref().is_some_and(|seen| *seen != now);
            *seen.borrow_mut() = Some(now);
            if moved {
                untrack(|| session.remeasure());
            }
        });
        scope.id()
    }

    /// Stops what waits on a stroke: the hold and the scroll.
    fn settle(&self) {
        self.resting.set(None);
        self.hold.borrow_mut().take();
        self.scroll.borrow_mut().take();
    }

    fn area_of(&self, id: DropAreaId) -> Option<Rc<DropArea<P>>> {
        self.areas
            .borrow()
            .iter()
            .find(|(at, _)| *at == id)
            .map(|(_, area)| Rc::clone(area))
    }

    /// The areas, the innermost first: one drawn inside another is offered the payload before it.
    fn areas_inside_out(&self) -> Vec<(DropAreaId, Rc<DropArea<P>>)> {
        let mut areas = self.areas.borrow().clone();
        areas.sort_by_cached_key(|(_, area)| Reverse(placement_depth(area.node)));
        areas
    }

    /// Where `payload` would land with the pointer at `at` on the surface, and which item the pointer rests on.
    fn read(&self, payload: &P, at: (f32, f32)) -> Reading {
        let under: Vec<(DropAreaId, Rc<DropArea<P>>, (f32, f32), AreaLayout)> = self
            .areas_inside_out()
            .into_iter()
            .filter_map(|(id, area)| {
                let local = from_surface(area.node, at, true)?;
                let layout = (area.layout)();
                reaches(&layout.bounds, local).then_some((id, area, local, layout))
            })
            .collect();
        let resting = under.first().and_then(|(id, area, local, layout)| {
            item_under(&layout.items, area.axis, *local).map(|index| (*id, index))
        });
        let over = under.iter().find_map(|(id, area, local, layout)| {
            place_at(area, payload, layout, *local).map(|place| (*id, place))
        });
        Reading { over, resting }
    }

    /// Starts waiting for the hold on the item the pointer now rests on, when that is another item than before.
    fn rest(self: &Rc<Self>, resting: Option<(DropAreaId, usize)>) {
        if self.resting.get() == resting {
            return;
        }
        self.resting.set(resting);
        let timer = resting.and_then(|(id, index)| {
            self.area_of(id)?.on_hold.as_ref()?;
            let session = Rc::downgrade(self);
            Some(run_after(self.hold_delay.get(), move || {
                if let Some(session) = session.upgrade() {
                    session.held(id, index);
                }
            }))
        });
        *self.hold.borrow_mut() = timer;
    }

    fn held(&self, id: DropAreaId, index: usize) {
        if self.resting.get() != Some((id, index)) {
            return;
        }
        let Some(stroke) = self.stroke.peek() else {
            return;
        };
        if let Some(on_hold) = self.area_of(id).and_then(|area| area.on_hold.clone()) {
            on_hold(&stroke.payload, index);
        }
    }

    /// Starts scrolling when the pointer at `at` holds a scroll area near its edge, and stops when it no longer does.
    fn autoscroll(self: &Rc<Self>, at: (f32, f32)) {
        let wanted = self.scroll_at(at).is_some();
        let running = self.scroll.borrow().is_some();
        match (wanted, running) {
            (true, false) => {
                self.scrolled_at.set(timer_now());
                let session = Rc::downgrade(self);
                let timer = run_after(SCROLL_TICK, move || {
                    if let Some(session) = session.upgrade() {
                        session.scroll_step();
                    }
                });
                *self.scroll.borrow_mut() = Some(timer);
            }
            (false, true) => {
                self.scroll.borrow_mut().take();
            }
            _ => {}
        }
    }

    fn scroll_step(self: &Rc<Self>) {
        self.scroll.borrow_mut().take();
        let Some(stroke) = self.stroke.peek() else {
            return;
        };
        let Some((viewport, (across, down))) = self.scroll_at(stroke.at) else {
            return;
        };
        let now = timer_now();
        let seconds = now
            .saturating_duration_since(self.scrolled_at.get())
            .min(MAX_SCROLL_STEP)
            .as_secs_f32();
        let (x, y) = viewport.peek_offset();
        let (max_x, max_y) = viewport.peek_ranges().unwrap_or_default();
        viewport.scroll_to(
            (x + across * seconds).clamp(0.0, max_x),
            (y + down * seconds).clamp(0.0, max_y),
        );
        // The content moved under a pointer that did not, so what it is over is measured again.
        self.update(stroke);
    }

    /// The scroll area an area of the session sits in that the pointer at `at` holds near an edge it can still scroll towards, innermost first, and how fast it scrolls across and down.
    fn scroll_at(&self, at: (f32, f32)) -> Option<(ScrollViewport, (f32, f32))> {
        let mut seen: Vec<NodeId> = Vec::new();
        for (_, area) in self.areas_inside_out() {
            for viewport in scroll_viewports_of(area.node) {
                if seen.contains(&viewport.area()) {
                    continue;
                }
                seen.push(viewport.area());
                let Some(rect) = visible_rect(viewport.area()) else {
                    continue;
                };
                if !rect.contains(at.0, at.1) {
                    continue;
                }
                let Some((max_x, max_y)) = viewport.peek_ranges() else {
                    continue;
                };
                let (x, y) = viewport.peek_offset();
                let across = edge_speed(
                    at.0 - rect.x,
                    rect.x + rect.width - at.0,
                    rect.width,
                    x,
                    max_x,
                );
                let down = edge_speed(
                    at.1 - rect.y,
                    rect.y + rect.height - at.1,
                    rect.height,
                    y,
                    max_y,
                );
                if across != 0.0 || down != 0.0 {
                    return Some((viewport, (across, down)));
                }
            }
        }
        None
    }
}

/// How fast a scroll area `extent` long, scrolled to `offset` of `range`, scrolls along one axis with the pointer `from_start` past its start and `from_end` short of its end: back towards the start near the start, on towards the end near the end, faster the closer to the edge.
fn edge_speed(from_start: f32, from_end: f32, extent: f32, offset: f32, range: f32) -> f32 {
    let band = SCROLL_EDGE.min(extent / 4.0);
    if band <= 0.0 {
        return 0.0;
    }
    if from_start < band && offset > 0.0 {
        -SCROLL_SPEED * (1.0 - from_start / band)
    } else if from_end < band && offset < range {
        SCROLL_SPEED * (1.0 - from_end / band)
    } else {
        0.0
    }
}

/// Where the item `from` reports a point `local` to it: in the space the item is laid out in, and on the surface.
fn surface_point(from: NodeId, local: (f32, f32)) -> Option<((f32, f32), (f32, f32))> {
    let laid = track_layout(from)?.peek();
    let point = (laid.x + local.0, laid.y + local.1);
    let at = match placed_in(from) {
        Some(parent) => to_surface(parent, point),
        None => point,
    };
    Some((point, at))
}

/// A point on the surface in the space the box `space` lays its children out in, or in the surface's own for `None`: `None` only where a clip hides it, when `clipped`, or a transform flattened the space.
fn in_space(space: Option<NodeId>, at: (f32, f32), clipped: bool) -> Option<(f32, f32)> {
    match space {
        Some(space) => from_surface(space, at, clipped),
        None => Some(at),
    }
}

/// Whether `at` is inside `bounds` or on its edge: a drag kept inside an area reports its far edge, and that is still the area.
fn reaches(bounds: &Rect, at: (f32, f32)) -> bool {
    (bounds.x..=bounds.x + bounds.width).contains(&at.0)
        && (bounds.y..=bounds.y + bounds.height).contains(&at.1)
}

/// The start and the length of `rect` along `axis`.
fn span(axis: Axis, rect: &Rect) -> (f32, f32) {
    match axis {
        Axis::Horizontal => (rect.x, rect.width),
        Axis::Vertical => (rect.y, rect.height),
    }
}

/// The item whose span along `axis` holds the point `at`.
fn item_under(items: &[Rect], axis: Axis, at: (f32, f32)) -> Option<usize> {
    let along = axis.of(at);
    items.iter().position(|rect| {
        let (start, length) = span(axis, rect);
        start <= along && along < start + length
    })
}

/// Where in `area`, laid out as `layout`, `payload` lands with the pointer at `at`, or `None` where the area accepts it nowhere near.
///
/// Over an item that accepts the payload inside, the middle half of it is inside and each outer quarter the gap on that side; over one that does not, each half is the gap on its side. Between the items, it is the gap the pointer is in. A place the area refuses gives way to the other one the pointer is near, and a gap refused next to an item refused inside is no place at all.
fn place_at<P>(
    area: &DropArea<P>,
    payload: &P,
    layout: &AreaLayout,
    at: (f32, f32),
) -> Option<DropPlace<usize>> {
    let accepts = |place| (area.accepts)(payload, place);
    let Some(index) = item_under(&layout.items, area.axis, at) else {
        let gap = DropPlace::Between(insertion_index(&layout.items, at, area.axis));
        return accepts(gap).then_some(gap);
    };
    let along = area.axis.of(at);
    let (start, length) = span(area.axis, &layout.items[index]);
    let inside = DropPlace::Inside(index);
    if !accepts(inside) {
        let gap = DropPlace::Between(if along < start + length / 2.0 {
            index
        } else {
            index + 1
        });
        return accepts(gap).then_some(gap);
    }
    let edge = length / 4.0;
    let gap = if along < start + edge {
        DropPlace::Between(index)
    } else if along >= start + length - edge {
        DropPlace::Between(index + 1)
    } else {
        return Some(inside);
    };
    Some(if accepts(gap) { gap } else { inside })
}

#[cfg(test)]
#[path = "drag_session.test.rs"]
mod tests;
