use super::*;
use geometry_core::LayoutGrid;

fn border_of(style: LayoutStyle) -> (f32, f32) {
    border_on(style, Size::ZERO)
}

fn border_on(style: LayoutStyle, surface: Size) -> (f32, f32) {
    let resolved = style.resolve(Direction::Ltr, surface);
    (
        length_of(resolved.border.left),
        length_of(resolved.border.top),
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

/// Padding written as a fraction of the surface has no value until the surface is known, so the frame has to be reserved from the padding it resolved to — read at the builder, it was zero and every such box paid a cell it already had.
#[test]
fn a_surface_fraction_of_padding_counts_once_it_has_resolved() {
    let style = LayoutStyle::new()
        .padding_all(SizeDimension::SurfaceWidth(0.02))
        .bordered_on(CELL);
    assert_eq!(
        border_on(style.clone(), Size::new(800.0, 400.0)),
        (0.0, 0.0),
        "16px of padding is a cell on both axes"
    );
    assert_eq!(
        border_on(style, Size::new(200.0, 400.0)),
        (4.0, 12.0),
        "4px of padding leaves the rest of each cell to reserve"
    );
}

/// The logical edges resolve with the direction, so the reservation has to wait for them too — and a builder chain no longer has to put `bordered` last.
#[test]
fn logical_padding_counts_whichever_side_it_lands_on() {
    let style = LayoutStyle::new()
        .bordered_on(CELL)
        .padding_start(8.0)
        .padding_vertical(16.0);
    let rtl = style.resolve(Direction::Rtl, Size::ZERO);
    assert_eq!(length_of(rtl.border.right), 0.0);
    assert_eq!(length_of(rtl.border.left), 8.0);
    assert_eq!(length_of(rtl.border.top), 0.0);
}
