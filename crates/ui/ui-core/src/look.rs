//! [`Look`]: what a box painted from one value looks like, the value a view hands a box with `look:`.

use renderer_core::Color;

/// How a box painted from one value looks at rest, under the pointer and pressed, and the ink of what it holds, each worked out from `around`, the ink of the text around the box.
///
/// A view hands one to a `row`, `col`, `grid` or `box` with `look:`, which reads `around` from the box's text and repaints as it changes, as it does when anything a method reads changes. The box draws the frame at the width it declares, and takes the pointer states only where it answers the pointer anyway.
///
/// Every method has a default that paints nothing of its own, so a look says only what it paints: no fill, no frame, the ink around kept, and no change under the pointer.
pub trait Look {
    /// The fill at rest.
    fn fill(&self, _around: Color) -> Color {
        Color::TRANSPARENT
    }

    /// The colour of the frame.
    fn frame(&self, _around: Color) -> Color {
        Color::TRANSPARENT
    }

    /// The ink of what sits on the box, which everything inside it inherits.
    fn ink(&self, around: Color) -> Color {
        around
    }

    /// The fill while the pointer rests on the box.
    fn hovered(&self, around: Color) -> Color {
        self.fill(around)
    }

    /// The fill while the box is pressed.
    fn pressed(&self, around: Color) -> Color {
        self.hovered(around)
    }
}

#[cfg(test)]
#[path = "look.test.rs"]
mod tests;
