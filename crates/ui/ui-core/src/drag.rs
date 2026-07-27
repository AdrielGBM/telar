use geometry_core::Rect;
use platform_core::{Event, PointerButton};
use ui_tree::EventResult;

/// A drag gesture any container can opt into: reports the pointer position on a press inside its bounds
/// and on every move until release. Because pointer events are broadcast to every widget, a drag keeps
/// receiving moves even after the pointer leaves the widget's bounds — no explicit pointer capture is
/// needed. Coordinates are reported *local to the widget* (relative to its rect origin), so a slider maps
/// `x / width` to a value regardless of where the widget sits; they can go negative or exceed the size
/// once the pointer leaves the bounds.
#[derive(Default)]
pub(crate) struct DragGesture {
    on_drag: Option<Box<dyn Fn(f32, f32)>>,
    on_drag_end: Option<Box<dyn Fn(f32, f32)>>,
    dragging: bool,
    /// The last position the drag reported, so an end with no event of its own still knows where it got to.
    last: (f32, f32),
}

impl DragGesture {
    pub(crate) fn set(&mut self, f: impl Fn(f32, f32) + 'static) {
        self.on_drag = Some(Box::new(f));
    }

    pub(crate) fn set_end(&mut self, f: impl Fn(f32, f32) + 'static) {
        self.on_drag_end = Some(Box::new(f));
    }

    pub(crate) fn is_set(&self) -> bool {
        self.on_drag.is_some() || self.on_drag_end.is_some()
    }

    /// A primary press inside `rect` starts the drag and reports the press point. Returns `Handled` when
    /// it starts, so the press is consumed (as with a tap).
    pub(crate) fn press(&mut self, event: &Event, rect: Rect) -> EventResult {
        if let Event::PointerPressed {
            x,
            y,
            button: PointerButton::Primary,
            ..
        } = event
            && rect.contains(*x as f32, *y as f32)
        {
            self.dragging = true;
            self.report(*x as f32 - rect.x, *y as f32 - rect.y);
            return EventResult::Handled;
        }
        EventResult::Ignored
    }

    /// While a drag is active, reports each move (local to `rect`). Returns `Handled` so it is consumed.
    pub(crate) fn moved(&mut self, event: &Event, rect: Rect) -> EventResult {
        if self.dragging
            && let Event::PointerMoved { x, y, .. } = event
        {
            self.report(*x as f32 - rect.x, *y as f32 - rect.y);
            return EventResult::Handled;
        }
        EventResult::Ignored
    }

    fn report(&mut self, x: f32, y: f32) {
        self.last = (x, y);
        if let Some(cb) = &self.on_drag {
            cb(x, y);
        }
    }

    /// Ends the drag (on release, or when the pointer leaves the window) and fires `on_drag_end` with where
    /// it finished. Returns whether one was active, so the caller can consume the release that ended it.
    ///
    /// `at` is the release position when the caller has one. The fallback matters: a drag also ends on
    /// `CursorLeft`, and on a child consuming the release, neither of which carries a position — reporting the
    /// last place the drag actually reached is the only answer that is true in all three cases.
    pub(crate) fn end(&mut self, at: Option<(f32, f32)>) -> bool {
        let was_dragging = std::mem::take(&mut self.dragging);
        if was_dragging {
            let (x, y) = at.unwrap_or(self.last);
            self.last = (x, y);
            if let Some(cb) = &self.on_drag_end {
                cb(x, y);
            }
        }
        was_dragging
    }
}
