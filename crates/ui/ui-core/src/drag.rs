//! The drag gesture: arming on a press, reporting each move, and deciding who owns the stroke.

use std::cell::Cell;

use geometry_core::Rect;
use platform_core::{Event, ModifiersState, PointerButton};
use ui_tree::EventResult;

use crate::pointer::PointerButtons;

/// What armed a drag: the button pressed, and what was held down at that moment.
///
/// Frozen at the press, and that is the whole of it. [`modifiers`](crate::modifiers) answers what is held *now*, so a mode read from it mid-stroke would change under a hand that let go of Shift — turning an orbit into a pan halfway through. A gesture chooses what it is once, when it starts, and is measured from there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DragStart {
    pub button: PointerButton,
    pub modifiers: ModifiersState,
}

thread_local! {
    /// What armed the drag whose callback is currently running, and how far it has travelled. Ambient rather than a callback parameter for the reason [`PointerButtons`] gives: widening `on_drag` would make the whole catalogue pay for a question two widgets ask.
    static ACTIVE: Cell<Option<(DragStart, f32)>> = const { Cell::new(None) };
    /// Whether the press being dispatched right now has already armed a drag somewhere below. See [`claimed`].
    static CLAIMED: Cell<bool> = const { Cell::new(false) };
}

/// Marks the press being dispatched as claimed by a drag. Called by every gesture that arms one.
pub(crate) fn claim() {
    CLAIMED.with(|c| c.set(true));
}

/// Runs `dispatch` over the children and reports whether one of them armed a drag on this press.
///
/// **The innermost drag owns the stroke.** A tab that reorders sits in a strip that moves the window, and a slider sits in a card that can be dragged around; with both armed, one press runs two gestures and the answer is whichever one the eye notices first. The parent asks this and stands its own down — it is the same rule the pointer already follows for hit-testing, said for gestures.
pub(crate) fn claimed<R>(dispatch: impl FnOnce() -> R) -> (R, bool) {
    let outer = CLAIMED.with(|c| c.replace(false));
    let answer = dispatch();
    let claimed = CLAIMED.with(|c| c.replace(outer || c.get()));
    (answer, claimed)
}

/// What armed the drag whose callback is running, or `None` outside one.
///
/// The button-and-modifier half of mode dispatch: a viewport reads this once and knows whether this stroke is an orbit, a pan or a dolly — without every drag callback in the catalogue growing a parameter for it.
pub fn drag_start() -> Option<DragStart> {
    ACTIVE.with(|a| a.get()).map(|(start, _)| start)
}

/// How far the drag whose callback is running has been from its press point, at its furthest.
///
/// The number a click-versus-drag decision is read against when the widget wants to make it itself rather than hand it to [`DragGesture`'s threshold](crate::StyledContainer::drag_threshold).
pub fn drag_travel() -> f32 {
    ACTIVE.with(|a| a.get()).map_or(0.0, |(_, travel)| travel)
}

/// Which way a drag is allowed to travel.
///
/// **A gesture with one meaning should not report two numbers.** A strip of tabs is reordered along its own axis and a slider has only one; without this each of them takes the pointer's other coordinate and throws it away, which is the same arithmetic written once per widget — and the ones that forget let what they are dragging wander off the line it lives on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DragAxis {
    #[default]
    Free,
    Horizontal,
    Vertical,
}

/// A drag gesture any container can opt into: reports the pointer position on a press inside its bounds and on every move until release. Because pointer events are broadcast to every widget, a drag keeps receiving moves even after the pointer leaves the widget's bounds — no explicit pointer capture is needed. Coordinates are reported *local to the widget* (relative to its rect origin), so a slider maps `x / width` to a value regardless of where the widget sits; they can go negative or exceed the size once the pointer leaves the bounds.
pub(crate) struct DragGesture {
    on_drag: Option<Box<dyn Fn(f32, f32)>>,
    on_drag_end: Option<Box<dyn Fn(f32, f32)>>,
    /// Which buttons may start it. The primary one alone by default, which is every slider and splitter in the catalogue; a viewport widens it, because a modeller pans with the button the OS calls secondary.
    arms: PointerButtons,
    /// Where the press landed (widget-local) and what armed it, for as long as the gesture is live.
    origin: Option<((f32, f32), DragStart)>,
    /// How far the pointer must travel before this counts as a drag at all.
    ///
    /// `0.0` (the default) reports from the press, which is what a slider wants: pressing the track *is* setting the value, and waiting for movement would make the first click do nothing. A viewport wants the other reading, where a press that never travelled was a click on whatever sits under it. Both are legitimate, so it is the caller's to say — and the widget that says nothing keeps what it always had.
    threshold: f32,
    /// The furthest the pointer has been from the press.
    travel: f32,
    /// Whether the threshold has been cleared, so the callbacks are running.
    started: bool,
    /// The last position the drag reported, so an end with no event of its own still knows where it got to.
    last: (f32, f32),
    /// Which way it may travel. The other coordinate is reported as it was at the press, so a widget on a line stays on it.
    axis: DragAxis,
    /// The box the reported point is kept inside, **in the widget's own coordinates**, or `None` for a drag that may be reported anywhere the pointer goes.
    ///
    /// A drag keeps receiving moves after the pointer leaves the widget — that is what makes a slider still track when the hand overshoots — and the same broadcast is what lets a pointer that left the window report a position no layout could ever produce. Bounding it is the caller saying where the answer is allowed to be, once, instead of clamping it at every use.
    within: Option<Box<dyn Fn() -> Rect>>,
}

impl Default for DragGesture {
    fn default() -> Self {
        Self {
            on_drag: None,
            on_drag_end: None,
            arms: PointerButtons {
                primary: true,
                ..PointerButtons::default()
            },
            origin: None,
            threshold: 0.0,
            travel: 0.0,
            started: false,
            last: (0.0, 0.0),
            axis: DragAxis::Free,
            within: None,
        }
    }
}

impl DragGesture {
    pub(crate) fn set(&mut self, f: impl Fn(f32, f32) + 'static) {
        self.on_drag = Some(Box::new(f));
    }

    pub(crate) fn set_end(&mut self, f: impl Fn(f32, f32) + 'static) {
        self.on_drag_end = Some(Box::new(f));
    }

    pub(crate) fn arm_with(&mut self, button: &PointerButton) {
        self.arms = self.arms.with(button);
    }

    /// Whether `button` may start this drag.
    pub(crate) fn arms(&self, button: &PointerButton) -> bool {
        self.arms.holds(button)
    }

    pub(crate) fn is_set(&self) -> bool {
        self.on_drag.is_some() || self.on_drag_end.is_some()
    }

    pub(crate) fn lock_to(&mut self, axis: DragAxis) {
        self.axis = axis;
    }

    pub(crate) fn keep_within(&mut self, within: impl Fn() -> Rect + 'static) {
        self.within = Some(Box::new(within));
    }

    /// The point as the callbacks are told it: on the axis it is allowed to travel, and inside the box it is allowed to be. Applied once, here, so every reader of `on_drag`, `on_drag_end` and the travel measured against the threshold sees the same answer.
    fn held(&self, local: (f32, f32)) -> (f32, f32) {
        let start = self.origin.map(|(at, _)| at).unwrap_or(local);
        let (x, y) = match self.axis {
            DragAxis::Free => local,
            DragAxis::Horizontal => (local.0, start.1),
            DragAxis::Vertical => (start.0, local.1),
        };
        match &self.within {
            Some(box_of) => {
                let bounds = box_of();
                (
                    x.clamp(bounds.x, bounds.x + bounds.width),
                    y.clamp(bounds.y, bounds.y + bounds.height),
                )
            }
            None => (x, y),
        }
    }

    pub(crate) fn set_threshold(&mut self, px: f32) {
        self.threshold = px.max(0.0);
    }

    /// Whether the gesture has cleared its threshold and is reporting. A widget with both a tap and a thresholded drag uses this to drop the tap once the stroke has committed to being a drag.
    pub(crate) fn has_started(&self) -> bool {
        self.started
    }

    pub(crate) fn has_threshold(&self) -> bool {
        self.threshold > 0.0
    }

    /// A press inside `rect` with a button this gesture arms starts the drag and reports the press point. Returns `Handled` when it starts, so the press is consumed (as with a tap).
    ///
    /// With a threshold set, the press *arms* the gesture without reporting: nothing has travelled yet, so nothing has been dragged.
    pub(crate) fn press(&mut self, event: &Event, rect: Rect) -> EventResult {
        if let Event::PointerPressed { x, y, button, .. } = event
            && self.arms(button)
            && rect.contains(*x as f32, *y as f32)
        {
            let local = (*x as f32 - rect.x, *y as f32 - rect.y);
            self.origin = Some((
                local,
                DragStart {
                    button: *button,
                    modifiers: crate::keyboard::modifiers(),
                },
            ));
            self.travel = 0.0;
            self.started = !self.has_threshold();
            // Before anything else runs, so whatever contains this box can ask whether the stroke was already spoken for.
            claim();
            if self.started {
                self.report(local.0, local.1);
            }
            return EventResult::Handled;
        }
        EventResult::Ignored
    }

    /// While a drag is active, reports each move (local to `rect`). Returns `Handled` so it is consumed — and `Ignored` while the gesture is armed but has not travelled far enough to be a drag yet.
    pub(crate) fn moved(&mut self, event: &Event, rect: Rect) -> EventResult {
        let (Some((press, _)), Event::PointerMoved { x, y, .. }) = (self.origin, event) else {
            return EventResult::Ignored;
        };
        let local = (*x as f32 - rect.x, *y as f32 - rect.y);
        let (dx, dy) = (local.0 - press.0, local.1 - press.1);
        self.travel = self.travel.max(dx.hypot(dy));
        // The drag begins here, not back at the press: reporting the press point retroactively would jump the dragged thing by the slop distance the moment it started moving.
        self.started |= self.travel > self.threshold;
        if !self.started {
            return EventResult::Ignored;
        }
        self.report(local.0, local.1);
        EventResult::Handled
    }

    fn report(&mut self, x: f32, y: f32) {
        // At the one place every report goes through, so `on_drag_end` and a `last` read after a stroke that left the window answer the same as the moves did.
        let (x, y) = self.held((x, y));
        self.last = (x, y);
        let Some((_, start)) = self.origin else {
            return;
        };
        if let Some(cb) = &self.on_drag {
            in_drag(start, self.travel, || cb(x, y));
        }
    }

    /// Ends the drag (on release, or when the pointer leaves the window) and fires `on_drag_end` with where it finished. Returns whether one was active, so the caller can consume the release that ended it.
    ///
    /// `at` is the release position when the caller has one. The fallback matters: a drag also ends on `CursorLeft`, and on a child consuming the release, neither of which carries a position — reporting the last place the drag actually reached is the only answer that is true in all three cases. A gesture that never cleared its threshold ends silently and answers `false`: nothing was dragged, so the release belongs to whatever else the widget arms — which is how a click and a drag on one button stop being ambiguous.
    pub(crate) fn end(&mut self, at: Option<(f32, f32)>) -> bool {
        // Held before the origin goes: the axis is measured from where the press was.
        let landed = at.map(|at| self.held(at));
        let Some((_, start)) = self.origin.take() else {
            return false;
        };
        let was_dragging = std::mem::take(&mut self.started);
        if was_dragging {
            let (x, y) = landed.unwrap_or(self.last);
            self.last = (x, y);
            if let Some(cb) = &self.on_drag_end {
                in_drag(start, self.travel, || cb(x, y));
            }
        }
        was_dragging
    }
}

/// Runs `f` with [`drag_start`] and [`drag_travel`] answering for this gesture. The previous value is put back rather than cleared, so a drag callback that builds a widget which drags in turn does not blank the outer.
fn in_drag<R>(start: DragStart, travel: f32, f: impl FnOnce() -> R) -> R {
    let outer = ACTIVE.with(|a| a.replace(Some((start, travel))));
    let out = f();
    ACTIVE.with(|a| a.set(outer));
    out
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use platform_core::PointerSource;

    use super::*;

    const RECT: Rect = Rect {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 100.0,
    };

    fn press_at(x: f32, y: f32) -> Event {
        Event::PointerPressed {
            x: x as f64,
            y: y as f64,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        }
    }

    fn move_to(x: f32, y: f32) -> Event {
        Event::PointerMoved {
            x: x as f64,
            y: y as f64,
            source: PointerSource::Mouse,
        }
    }

    /// Records every position the gesture reported, and what it said armed the stroke at the time.
    type Log = Rc<RefCell<Vec<((f32, f32), Option<DragStart>)>>>;

    fn logging(threshold: f32) -> (DragGesture, Log) {
        let log: Log = Rc::new(RefCell::new(Vec::new()));
        let mut drag = DragGesture::default();
        drag.set_threshold(threshold);
        let sink = log.clone();
        drag.set(move |x, y| sink.borrow_mut().push(((x, y), drag_start())));
        (drag, log)
    }

    /// The default, which a slider depends on: pressing the track *is* setting the value, so the press itself reports and waiting for movement would make the first click do nothing.
    #[test]
    fn without_a_threshold_the_press_itself_reports() {
        let (mut drag, log) = logging(0.0);
        assert_eq!(
            drag.press(&press_at(30.0, 40.0), RECT),
            EventResult::Handled
        );
        assert_eq!(log.borrow().len(), 1, "the press reported straight away");
        assert_eq!(log.borrow()[0].0, (30.0, 40.0));
    }

    /// And the reading a viewport needs: a stroke that never travelled was a click on whatever sits under it, not a drag of nothing. `end` answering `false` is what leaves the release to the tap gesture.
    #[test]
    fn a_press_that_never_travels_is_not_a_drag() {
        let (mut drag, log) = logging(4.0);
        drag.press(&press_at(30.0, 40.0), RECT);
        drag.moved(&move_to(32.0, 41.0), RECT);
        assert!(log.borrow().is_empty(), "two pixels is not a drag");
        assert!(!drag.end(None), "so nothing was dragged to end");
    }

    /// Crossing the threshold starts the drag *where it crossed*. Reporting the press point retroactively would jump whatever is being dragged by the slop distance the instant it started moving.
    #[test]
    fn crossing_the_threshold_starts_the_drag_where_it_crossed() {
        let (mut drag, log) = logging(4.0);
        drag.press(&press_at(30.0, 40.0), RECT);
        drag.moved(&move_to(32.0, 40.0), RECT);
        drag.moved(&move_to(50.0, 40.0), RECT);

        assert_eq!(log.borrow().len(), 1, "only the move that cleared it");
        assert_eq!(log.borrow()[0].0, (50.0, 40.0), "and not back at the press");
        assert!(drag.end(None), "this one really was a drag");
    }

    /// Mode dispatch, and the reason it is frozen: a hand that lets go of Shift halfway through would turn an orbit into a pan mid-stroke if the gesture asked what is held *now*.
    #[test]
    fn a_drag_reports_what_armed_it_and_not_what_is_held_now() {
        crate::keyboard::reset();
        crate::keyboard::observe(&Event::ModifiersChanged {
            modifiers: ModifiersState {
                is_shift: true,
                ..Default::default()
            },
        });
        let (mut drag, log) = logging(0.0);
        drag.press(&press_at(10.0, 10.0), RECT);

        crate::keyboard::observe(&Event::ModifiersChanged {
            modifiers: ModifiersState::default(),
        });
        drag.moved(&move_to(40.0, 10.0), RECT);
        crate::keyboard::reset();

        let entries = log.borrow();
        assert!(
            entries.iter().all(|(_, start)| start
                .is_some_and(|s| s.modifiers.is_shift && s.button == PointerButton::Primary)),
            "every report names the press, including the one after Shift was released: {entries:?}"
        );
        assert_eq!(
            drag_start(),
            None,
            "and nothing leaks out of the callback it was scoped to"
        );
    }
}
