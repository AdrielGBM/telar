use geometry_core::Rect;
use platform_core::{Event, PointerButton};
use ui_tree::EventResult;

/// Max pointer travel (logical px) from the press point still counted as a tap rather than a scroll/drag.
const TAP_SLOP: f32 = 10.0;

/// A press ("tap") gesture that any container can opt into. Mirrors `Button`'s touch/scroll
/// disambiguation without the button's hover/label concerns: the callback fires on release, not
/// press, and is cancelled once the pointer travels past `TAP_SLOP` — so a scroll gesture that
/// begins on the widget never triggers it. Hit-testing is the caller's job (it owns its layout rect).
#[derive(Default)]
pub(crate) struct PressGesture {
    on_press: Option<Box<dyn Fn()>>,
    // The press point while a tap is pending; cleared once the pointer travels past TAP_SLOP.
    press_origin: Option<(f32, f32)>,
}

impl PressGesture {
    pub(crate) fn set(&mut self, f: impl Fn() + 'static) {
        self.on_press = Some(Box::new(f));
    }

    pub(crate) fn is_set(&self) -> bool {
        self.on_press.is_some()
    }

    /// Past the slop the gesture is a scroll/drag, not a tap: drop the pending press so the release
    /// won't fire. This is what stops a scroll begun on the widget from clicking it.
    pub(crate) fn track_move(&mut self, event: &Event) {
        if let (Some((ox, oy)), Event::PointerMoved { x, y, .. }) = (self.press_origin, event) {
            let (dx, dy) = (*x as f32 - ox, *y as f32 - oy);
            if dx * dx + dy * dy > TAP_SLOP * TAP_SLOP {
                self.press_origin = None;
            }
        }
    }

    /// Arm a candidate tap if a primary press lands inside `rect`. Returns `Handled` when armed so the
    /// press is consumed (it does not fall through to widgets behind).
    pub(crate) fn arm(&mut self, event: &Event, rect: Rect) -> EventResult {
        if let Event::PointerPressed {
            x,
            y,
            button: PointerButton::Primary,
            ..
        } = event
            && rect.contains(*x as f32, *y as f32)
        {
            self.press_origin = Some((*x as f32, *y as f32));
            return EventResult::Handled;
        }
        EventResult::Ignored
    }

    /// Complete the tap: a primary release still inside `rect` (a drag past the slop already cleared
    /// the origin) fires the callback.
    pub(crate) fn release(&mut self, event: &Event, rect: Rect) -> EventResult {
        if let Event::PointerReleased {
            x,
            y,
            button: PointerButton::Primary,
            ..
        } = event
        {
            let armed = self.press_origin.take().is_some();
            if armed && rect.contains(*x as f32, *y as f32) {
                if let Some(cb) = &self.on_press {
                    cb();
                }
                return EventResult::Handled;
            }
        }
        EventResult::Ignored
    }

    /// Drop any pending tap (a child consumed the press, or the cursor left).
    pub(crate) fn cancel(&mut self) {
        self.press_origin = None;
    }
}
