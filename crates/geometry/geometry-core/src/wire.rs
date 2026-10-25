//! The byte spelling of the geometry that crosses between a host and a guest.

use wire_core::wire_struct;

use crate::{BorderRadius, Color, Insets, LayoutGrid, Point, Rect, Size};

wire_struct!(Point { x, y });
wire_struct!(Size { width, height });
wire_struct!(Rect {
    x,
    y,
    width,
    height
});
wire_struct!(Color { r, g, b, a });
wire_struct!(BorderRadius {
    top_left,
    top_right,
    bottom_right,
    bottom_left
});
wire_struct!(LayoutGrid { x, y });
wire_struct!(Insets {
    top,
    right,
    bottom,
    left
});

#[cfg(test)]
#[path = "wire_test.rs"]
mod tests;
