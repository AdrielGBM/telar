use std::time::{Duration, Instant};

use geometry_core::Rect;
use platform_core::{Event, PointerButton};
use ui_tree::EventResult;

/// Max pointer travel (logical px) from the press point still counted as a tap rather than a scroll/drag.
const TAP_SLOP: f32 = 10.0;

/// How long a press must be held (without moving past `TAP_SLOP`) to count as a long press rather than a tap.
const LONG_PRESS_THRESHOLD: Duration = Duration::from_millis(500);

/// A press ("tap") gesture that any container can opt into. Mirrors `Button`'s touch/scroll
/// disambiguation without the button's hover/label concerns: the callback fires on release, not
/// press, and is cancelled once the pointer travels past `TAP_SLOP` — so a scroll gesture that
/// begins on the widget never triggers it. Hit-testing is the caller's job (it owns its layout rect).
#[derive(Default)]
pub(crate) struct PressGesture {
    on_press: Option<Box<dyn Fn()>>,
    on_long_press: Option<Box<dyn Fn()>>,
    on_alt_press: Option<Box<dyn Fn(PointerButton)>>,
    // The press point while a tap is pending; cleared once the pointer travels past TAP_SLOP.
    press_origin: Option<(f32, f32)>,
    // When the pending press started; cleared alongside `press_origin`. Used to detect a long press.
    press_started_at: Option<Instant>,
    // Which button armed the pending gesture, so a release by a different one completes nothing.
    armed_button: Option<PointerButton>,
}

impl PressGesture {
    pub(crate) fn set(&mut self, f: impl Fn() + 'static) {
        self.on_press = Some(Box::new(f));
    }

    /// Fires the press without a pointer, for a control activated from the keyboard. Reports whether there was
    /// anything to fire, so a key that activated nothing is left for whoever else wants it.
    pub(crate) fn activate(&self) -> bool {
        match &self.on_press {
            Some(cb) => {
                cb();
                true
            }
            None => false,
        }
    }

    pub(crate) fn set_long_press(&mut self, f: impl Fn() + 'static) {
        self.on_long_press = Some(Box::new(f));
    }

    pub(crate) fn set_alt_press(&mut self, f: impl Fn(PointerButton) + 'static) {
        self.on_alt_press = Some(Box::new(f));
    }

    pub(crate) fn is_set(&self) -> bool {
        self.on_press.is_some() || self.on_long_press.is_some() || self.on_alt_press.is_some()
    }

    /// Whether this gesture wants buttons beyond the primary one. A container asks before consuming a
    /// non-primary press, so one keeps falling through to the widgets behind unless something asked for it.
    pub(crate) fn wants_alt(&self) -> bool {
        self.on_alt_press.is_some()
    }

    /// Whether `button` has a callback to complete. Without this a box that set only `on_alt_press` would arm
    /// on a primary press — and swallow it — having asked for nothing of the sort.
    fn accepts(&self, button: &PointerButton) -> bool {
        match button {
            PointerButton::Primary => self.on_press.is_some() || self.on_long_press.is_some(),
            _ => self.on_alt_press.is_some(),
        }
    }

    /// Past the slop the gesture is a scroll/drag, not a tap: drop the pending press so the release
    /// won't fire. This is what stops a scroll begun on the widget from clicking it.
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

    /// Arm a candidate tap if a press this gesture accepts lands inside `rect`. Returns `Handled` when armed
    /// so the press is consumed (it does not fall through to widgets behind).
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

    /// Complete the tap: a release by the same button that armed it, still inside `rect` (a drag past the
    /// slop already cleared the origin), fires the matching callback — unless the hold already crossed the
    /// long-press threshold, in which case the release fires nothing (the long press consumed the gesture).
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
            if armed && rect.contains(*x as f32, *y as f32) {
                match armed_button {
                    Some(PointerButton::Primary) => {
                        if let Some(cb) = &self.on_press {
                            cb();
                        }
                    }
                    Some(other) => {
                        if let Some(cb) = &self.on_alt_press {
                            cb(other);
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

    // No timer/tick is threaded into gestures, so the threshold can't be caught the instant it elapses;
    // instead this is polled on the next pointer event after arming (a move or the release), which fires
    // it a bit late rather than at exactly `LONG_PRESS_THRESHOLD`. Disarms the pending tap on fire so the
    // release that follows (if any) doesn't also produce an `on_press`.
    fn check_long_press(&mut self) -> bool {
        if self.armed_button != Some(PointerButton::Primary) {
            return false;
        }
        if let Some(started) = self.press_started_at
            && started.elapsed() >= LONG_PRESS_THRESHOLD
        {
            self.cancel();
            if let Some(cb) = &self.on_long_press {
                cb();
            }
            return true;
        }
        false
    }
}
