//! The arithmetic every resize drag in the catalogue shares: the size a drag took hold of, and how far the pointer has travelled since the press.

use std::cell::Cell;

use telar::{Point, Rect, Size, drag_start};

#[derive(Clone, Copy)]
pub(crate) struct Grab {
    pub(crate) from: Size,
    origin: Point,
}

impl Grab {
    pub(crate) fn travel(&self, pointer: Point) -> (f32, f32) {
        (pointer.x - self.origin.x, pointer.y - self.origin.y)
    }
}

/// The grab a resize drag holds from its first report to its end. It is latched rather than measured on every move because the handle, and the size it was measured against, move under the gesture.
#[derive(Default)]
pub(crate) struct Latch(Cell<Option<Grab>>);

impl Latch {
    /// The grab of the drag reporting `local` inside a handle laid out at `rect`, and where the pointer is in the coordinates `rect` is in. The first report takes hold from `from()` and the press point, which a drag past its threshold reports apart from the pointer.
    pub(crate) fn hold(
        &self,
        rect: Rect,
        local: (f32, f32),
        from: impl FnOnce() -> Size,
    ) -> (Grab, Point) {
        let pointer = Point::new(rect.x + local.0, rect.y + local.1);
        let grab = self.0.get().unwrap_or_else(|| {
            let (x, y) = drag_start().map_or(local, |start| start.at);
            let held = Grab {
                from: from(),
                origin: Point::new(rect.x + x, rect.y + y),
            };
            self.0.set(Some(held));
            held
        });
        (grab, pointer)
    }

    pub(crate) fn release(&self) -> Option<Grab> {
        self.0.take()
    }
}
