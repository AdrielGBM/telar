//! The byte spelling of the geometry that crosses between a host and a guest.

use wire_core::{
    Reader, Shaped, Wire, WireError, Writer, fold, fold_shape, shape_named, wire_struct,
};

use crate::{BorderRadius, Color, Insets, LayoutGrid, Point, Rect, Size, Transform};

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
wire_struct!(Insets {
    top,
    right,
    bottom,
    left
});
wire_struct!(Transform { a, b, c, d, e, f });

impl Shaped for LayoutGrid {
    const SHAPE: u64 = {
        let hash = fold_shape(fold(shape_named("LayoutGrid"), b"x"), f32::SHAPE);
        fold_shape(fold(hash, b"y"), f32::SHAPE)
    };
}

impl Wire for LayoutGrid {
    fn put(&self, w: &mut Writer) {
        self.x().put(w);
        self.y().put(w);
    }
    fn take(r: &mut Reader<'_>) -> Result<Self, WireError> {
        let (x, y) = (f32::take(r)?, f32::take(r)?);
        LayoutGrid::new(x, y).map_err(|_| WireError::Invalid("layout grid"))
    }
}

#[cfg(test)]
#[path = "wire.test.rs"]
mod tests;
