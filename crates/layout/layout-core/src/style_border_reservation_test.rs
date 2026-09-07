use super::*;
use geometry_core::LayoutGrid;

fn border_of(style: LayoutStyle) -> (f32, f32) {
    (
        length_of(style.inner.border.left),
        length_of(style.inner.border.top),
    )
}

const CELL: LayoutGrid = LayoutGrid { x: 8.0, y: 16.0 };

/// The whole of the rule: a stroke needs one cell per side, and padding already supplies cells. Only the shortfall is reserved, so a roomy box does not pay twice for its own frame.
#[test]
fn a_border_reserves_only_what_the_padding_does_not_give() {
    assert_eq!(
        border_of(LayoutStyle::new().bordered_on(CELL)),
        (8.0, 16.0),
        "an unpadded box needs the whole cell"
    );
    assert_eq!(
        border_of(LayoutStyle::new().padding_all(16.0).bordered_on(CELL)),
        (0.0, 0.0),
        "a box padded a cell already has room for its frame"
    );
    assert_eq!(
        border_of(LayoutStyle::new().padding_all(32.0).bordered_on(CELL)),
        (0.0, 0.0),
        "and so does a roomier one"
    );
}

/// A surface that can put an edge anywhere draws a stroke inside the box for free, exactly as it did before any of this existed. Goes through `bordered`, since the unit grid is the default and the short-circuit is the part worth covering.
#[test]
fn a_unit_grid_reserves_nothing() {
    assert_eq!(border_of(LayoutStyle::new().bordered()), (0.0, 0.0));
}
