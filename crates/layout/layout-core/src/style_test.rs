use super::*;

#[test]
fn logical_padding_resolves_to_the_edge_the_direction_starts_from() {
    let style = LayoutStyle::new().padding_start(8.0).padding_end(2.0);
    let ltr = style.resolve(Direction::Ltr);
    assert_eq!(ltr.padding.left, LengthPercentage::length(8.0));
    assert_eq!(ltr.padding.right, LengthPercentage::length(2.0));
    let rtl = style.resolve(Direction::Rtl);
    assert_eq!(rtl.padding.right, LengthPercentage::length(8.0));
    assert_eq!(rtl.padding.left, LengthPercentage::length(2.0));
}

#[test]
fn a_physical_edge_is_left_alone_by_the_direction() {
    let style = LayoutStyle::new().padding_left(12.0);
    for direction in [Direction::Ltr, Direction::Rtl] {
        let resolved = style.resolve(direction);
        assert_eq!(resolved.padding.left, LengthPercentage::length(12.0));
        assert_eq!(resolved.padding.right, LengthPercentage::length(0.0));
    }
}

#[test]
fn resolving_twice_does_not_accumulate() {
    // The engine re-resolves from the same LayoutStyle on every flip, so resolution must be a pure function of the intent.
    let style = LayoutStyle::new().padding_start(8.0);
    let _ = style.resolve(Direction::Rtl);
    let back = style.resolve(Direction::Ltr);
    assert_eq!(back.padding.left, LengthPercentage::length(8.0));
    assert_eq!(back.padding.right, LengthPercentage::length(0.0));
}

#[test]
fn a_row_reverses_under_rtl_but_an_explicit_reverse_does_not_flip_back() {
    let row = LayoutStyle::new().flex_row();
    assert_eq!(
        row.resolve(Direction::Ltr).flex_direction,
        FlexDirection::Row
    );
    assert_eq!(
        row.resolve(Direction::Rtl).flex_direction,
        FlexDirection::RowReverse
    );
    let reversed = LayoutStyle::new().flex_row_reverse();
    for direction in [Direction::Ltr, Direction::Rtl] {
        assert_eq!(
            reversed.resolve(direction).flex_direction,
            FlexDirection::RowReverse,
            "an explicit reverse is physical"
        );
    }
}

#[test]
fn a_column_is_unaffected_by_direction() {
    let col = LayoutStyle::new().flex_column();
    for direction in [Direction::Ltr, Direction::Rtl] {
        assert_eq!(col.resolve(direction).flex_direction, FlexDirection::Column);
    }
}

/// The point of keeping the style a node was built from: a logical edge resolves against whichever direction is current, from the intent, rather than being un-swapped from a physical edge that no longer says which one it was.
#[test]
fn a_logical_edge_resolves_to_the_side_the_direction_chose() {
    let style = LayoutStyle::new().margin_inline_start(4.0);
    let ltr = style.resolve(Direction::Ltr).margin;
    assert_eq!(ltr.left, LengthPercentageAuto::length(4.0));
    assert_eq!(ltr.right, LengthPercentageAuto::length(0.0));

    let rtl = style.resolve(Direction::Rtl).margin;
    assert_eq!(rtl.right, LengthPercentageAuto::length(4.0));
    assert_eq!(rtl.left, LengthPercentageAuto::length(0.0));
}

#[test]
fn style_default_is_block() {
    let style = LayoutStyle::new();
    assert_eq!(style.inner.display, Display::Block);
}

#[test]
fn style_width_sets_dimension() {
    let style = LayoutStyle::new().width(120.0);
    assert_eq!(style.inner.size.width, Dimension::length(120.0));
}

#[test]
fn style_width_px_reads_back_length() {
    let style = LayoutStyle::new().width(120.0);
    assert_eq!(style.width_px(), Some(120.0));
    assert!(!style.is_width_auto(), "a width in px is not auto");
}

#[test]
fn style_width_px_none_for_percent_or_default() {
    assert_eq!(LayoutStyle::new().width_px(), None);
    assert!(
        LayoutStyle::new().is_width_auto(),
        "an undeclared width is auto"
    );
    let percent = LayoutStyle::new().width(SizeDimension::Percent(0.5));
    assert_eq!(percent.width_px(), None);
    assert!(!percent.is_width_auto(), "and a percentage width is not");
}

#[test]
fn style_width_percent_sets_dimension() {
    let style = LayoutStyle::new().width(SizeDimension::Percent(0.5));
    assert_eq!(style.inner.size.width, Dimension::percent(0.5));
}

#[test]
fn style_height_sets_dimension() {
    let style = LayoutStyle::new().height(80.0);
    assert_eq!(style.inner.size.height, Dimension::length(80.0));
}

#[test]
fn style_max_width_sets_dimension() {
    let style = LayoutStyle::new().max_width(200.0);
    assert_eq!(
        style.inner.max_size.width,
        LengthPercentageAuto::length(200.0)
    );
}

#[test]
fn style_max_height_sets_dimension() {
    let style = LayoutStyle::new().max_height(150.0);
    assert_eq!(
        style.inner.max_size.height,
        LengthPercentageAuto::length(150.0)
    );
}

#[test]
fn style_flex_basis_percent_sets_dimension() {
    let style = LayoutStyle::new().flex_basis(SizeDimension::Percent(0.5));
    assert_eq!(style.inner.flex_basis, Dimension::percent(0.5));
}

#[test]
fn style_flex_row_sets_direction() {
    let style = LayoutStyle::new().flex_row();
    assert_eq!(style.inner.flex_direction, FlexDirection::Row);
}

#[test]
fn style_flex_column_sets_direction() {
    let style = LayoutStyle::new().flex_column();
    assert_eq!(style.inner.flex_direction, FlexDirection::Column);
}

#[test]
fn style_align_items_center_sets_field() {
    let style = LayoutStyle::new().align_items(AlignItems::CENTER);
    assert_eq!(style.inner.align_items, Some(taffy::AlignItems::CENTER));
}

#[test]
fn style_justify_center_sets_field() {
    let style = LayoutStyle::new().justify_content(JustifyContent::CENTER);
    assert_eq!(
        style.inner.justify_content,
        Some(taffy::JustifyContent::CENTER)
    );
}

#[test]
fn style_default_impl_matches_new() {
    let style = LayoutStyle::default();
    assert_eq!(style.inner.display, Display::Block);
}

#[test]
fn style_padding_horizontal_sets_left_right() {
    let style = LayoutStyle::new().padding_horizontal(10.0);
    assert_eq!(style.inner.padding.left, LengthPercentage::length(10.0));
    assert_eq!(style.inner.padding.right, LengthPercentage::length(10.0));
}

#[test]
fn style_padding_vertical_sets_top_bottom() {
    let style = LayoutStyle::new().padding_vertical(8.0);
    assert_eq!(style.inner.padding.top, LengthPercentage::length(8.0));
    assert_eq!(style.inner.padding.bottom, LengthPercentage::length(8.0));
}

#[test]
fn style_padding_top_sets_field() {
    let style = LayoutStyle::new().padding_top(4.0);
    assert_eq!(style.inner.padding.top, LengthPercentage::length(4.0));
}

// The block pair is physical-by-construction: there is no vertical writing mode here, so block start is the top in every direction. The inline pair is the one a flip moves, and it is the one kept in `logical` for `resolve` to place.
#[test]
fn margin_writes_the_block_pair_directly_and_defers_the_inline_pair() {
    let style = LayoutStyle::new().margin(Margin::symmetric(6.0, 12.0));
    assert_eq!(style.inner.margin.top, LengthPercentageAuto::length(6.0));
    assert_eq!(style.inner.margin.bottom, LengthPercentageAuto::length(6.0));
    assert_eq!(style.logical.margin_start, Some(SizeDimension::Px(12.0)));
    assert_eq!(style.logical.margin_end, Some(SizeDimension::Px(12.0)));
}

// What `margin_from_left` exists for: an x already in physical viewport coordinates must not mirror.
#[test]
fn a_margin_from_the_left_stays_left_under_rtl() {
    let style = LayoutStyle::new().margin_from_left(20.0);
    let ltr = style.resolve(Direction::Ltr);
    let rtl = style.resolve(Direction::Rtl);
    assert_eq!(ltr.margin.left, LengthPercentageAuto::length(20.0));
    assert_eq!(rtl.margin.left, LengthPercentageAuto::length(20.0));
}
