//! The press gesture: taps, long presses, and the non-primary buttons a box may also answer.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;
use web_time::Instant;

use geometry_core::Rect;
use platform_core::{Event, PointerButton, TAP_SLOP};
use ui_tree::EventResult;

use crate::Handler;

/// How long a press must be held (without moving past `TAP_SLOP`) to count as a long press rather than a tap.
const LONG_PRESS_THRESHOLD: Duration = Duration::from_millis(500);

/// A press ("tap") gesture that any container can opt into. Mirrors `Button`'s touch/scroll disambiguation without the button's hover/label concerns: the callback fires on release, not press, and is cancelled once the pointer travels past `TAP_SLOP` — so a scroll gesture that begins on the widget never triggers it. Hit-testing is the caller's job (it owns its layout rect).
#[derive(Default)]
pub(crate) struct PressGesture {
    // Shared with the focus order, so a control activated without a pointer runs exactly what a tap runs.
    primary: Rc<Primary>,
    on_long_press: Option<Handler>,
    on_alt_press: Option<Handler<fn(PointerButton)>>,
    // The press point while a tap is pending; cleared once the pointer travels past `TAP_SLOP`.
    press_origin: Option<(f32, f32)>,
    // Cleared alongside `press_origin`, so a press that turned into a drag never fires as a long press.
    press_started_at: Option<Instant>,
    // Which button armed the pending gesture, so a release by a different one completes nothing.
    armed_button: Option<PointerButton>,
}

/// What a primary press fires: the box's own handler, then where its link goes.
#[derive(Default)]
struct Primary {
    on_press: RefCell<Option<Handler>>,
    // Where a link goes on the press, after `on_press` has run: a box that is a link and also does something keeps both.
    follow: RefCell<Option<Handler>>,
}

impl Primary {
    /// Reports whether there was anything to fire, so a key that activated nothing is left for whoever else wants it.
    fn fire(&self) -> bool {
        // Cloned out so no borrow is held while a handler runs and, say, gives the box a new handler.
        let on_press = self.on_press.borrow().clone();
        let follow = self.follow.borrow().clone();
        if let Some(on_press) = &on_press {
            on_press.run();
        }
        if let Some(follow) = &follow {
            follow.run();
        }
        on_press.is_some() || follow.is_some()
    }

    fn is_set(&self) -> bool {
        self.on_press.borrow().is_some() || self.follow.borrow().is_some()
    }
}

impl PressGesture {
    pub(crate) fn set(&mut self, on_press: Handler) {
        *self.primary.on_press.borrow_mut() = Some(on_press);
    }

    pub(crate) fn set_follow(&mut self, follow: Handler) {
        *self.primary.follow.borrow_mut() = Some(follow);
    }

    /// What activating the control without a pointer fires — from the keyboard or from assistive technology — reporting whether there was anything to fire. It runs the handlers in place when it is called, so it can be taken before they are set.
    pub(crate) fn activation(&self) -> impl Fn() -> bool + 'static {
        let primary = Rc::clone(&self.primary);
        move || primary.fire()
    }

    fn fire_primary(&self) -> bool {
        self.primary.fire()
    }

    /// Whether a primary press or the keyboard does something here: what makes a box a button, where an alt or a long press alone only adds a gesture to it.
    pub(crate) fn activates(&self) -> bool {
        self.primary.is_set()
    }

    pub(crate) fn set_long_press(&mut self, on_long_press: Handler) {
        self.on_long_press = Some(on_long_press);
    }

    pub(crate) fn set_alt_press(&mut self, on_alt_press: Handler<fn(PointerButton)>) {
        self.on_alt_press = Some(on_alt_press);
    }

    pub(crate) fn is_set(&self) -> bool {
        self.activates() || self.on_long_press.is_some() || self.on_alt_press.is_some()
    }

    /// Whether this gesture wants buttons beyond the primary one. A container asks before consuming a non-primary press, so one keeps falling through to the widgets behind unless something asked for it.
    pub(crate) fn wants_alt(&self) -> bool {
        self.on_alt_press.is_some()
    }

    /// Whether `button` has a callback to complete. Without this a box that set only `on_alt_press` would arm on a primary press — and swallow it — having asked for nothing of the sort.
    fn accepts(&self, button: &PointerButton) -> bool {
        match button {
            PointerButton::Primary => self.activates() || self.on_long_press.is_some(),
            _ => self.on_alt_press.is_some(),
        }
    }

    /// Past the slop the gesture is a scroll/drag, not a tap: drop the pending press so the release won't fire. This is what stops a scroll begun on the widget from clicking it.
    pub(crate) fn track_move(&mut self, event: &Event) {
        if let (Some((ox, oy)), Event::PointerMoved { x, y, .. }) = (self.press_origin, event) {
            let (dx, dy) = (*x as f32 - ox, *y as f32 - oy);
            if dx * dx + dy * dy > TAP_SLOP * TAP_SLOP {
                self.cancel();
                return;
            }
            self.check_long_press();
        }
    }

    /// Arm a candidate tap if a press this gesture accepts lands inside `rect`. Returns `Handled` when armed so the press is consumed (it does not fall through to widgets behind).
    pub(crate) fn arm(&mut self, event: &Event, rect: Rect) -> EventResult {
        if let Event::PointerPressed { x, y, button, .. } = event
            && self.accepts(button)
            && rect.contains(*x as f32, *y as f32)
        {
            self.press_origin = Some((*x as f32, *y as f32));
            self.press_started_at = Some(Instant::now());
            self.armed_button = Some(*button);
            return EventResult::Handled;
        }
        EventResult::Ignored
    }

    /// Complete the tap: a release by the same button that armed it, still inside `rect` (a drag past the slop already cleared the origin) and not over something drawn in front of it ([`crate::pointer::pointer_occluded`]: a sibling on top, or the part of it a viewport has scrolled out of view, where the box already showed the press cancelled), fires the matching callback — unless the hold already crossed the long-press threshold, in which case the release fires nothing (the long press consumed the gesture).
    pub(crate) fn release(&mut self, event: &Event, rect: Rect) -> EventResult {
        if let Event::PointerReleased { x, y, button, .. } = event {
            if self.armed_button.as_ref() != Some(button) {
                return EventResult::Ignored;
            }
            if self.check_long_press() {
                return EventResult::Handled;
            }
            let armed = self.press_origin.take().is_some();
            self.press_started_at = None;
            let armed_button = self.armed_button.take();
            if armed && rect.contains(*x as f32, *y as f32) && !crate::pointer::pointer_occluded() {
                match armed_button {
                    Some(PointerButton::Primary) => {
                        self.fire_primary();
                    }
                    Some(other) => {
                        if let Some(cb) = &self.on_alt_press {
                            cb.run(other);
                        }
                    }
                    None => {}
                }
                return EventResult::Handled;
            }
        }
        EventResult::Ignored
    }

    /// Drop any pending tap (a child consumed the press, or the cursor left).
    pub(crate) fn cancel(&mut self) {
        self.press_origin = None;
        self.press_started_at = None;
        self.armed_button = None;
    }

    // No timer is threaded into gestures, so this is polled on the next pointer event after arming and fires a little late. Disarms the pending tap on fire, so the release that follows produces no `on_press`.
    fn check_long_press(&mut self) -> bool {
        if self.armed_button != Some(PointerButton::Primary) {
            return false;
        }
        if let Some(started) = self.press_started_at
            && started.elapsed() >= LONG_PRESS_THRESHOLD
        {
            self.cancel();
            if let Some(on_long_press) = &self.on_long_press {
                on_long_press.run();
            }
            return true;
        }
        false
    }
}
