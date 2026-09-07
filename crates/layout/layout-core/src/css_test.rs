use super::*;
use crate::style::{AlignItems, JustifyContent, SizeDimension};

pub(crate) fn css(style: LayoutStyle) -> String {
    style.to_css(Direction::Ltr).into_string()
}

/// Every property CSS already starts where taffy does is left unsaid, so what reaches the browser is what the app actually asked for.
#[test]
fn a_plain_style_says_only_what_it_is() {
    assert_eq!(
        css(LayoutStyle::new()),
        "display:block;box-sizing:border-box;position:relative;"
    );
}

#[test]
fn a_row_becomes_a_flex_row() {
    assert!(
        css(LayoutStyle::new().flex_row()).starts_with(
            "display:flex;box-sizing:border-box;position:relative;flex-direction:row;"
        ),
        "{}",
        css(LayoutStyle::new().flex_row())
    );
}

#[test]
fn a_row_reverses_under_rtl() {
    let style = LayoutStyle::new().flex_row();
    assert!(
        style
            .to_css(Direction::Rtl)
            .as_str()
            .contains("flex-direction:row-reverse"),
        "got {}",
        style.to_css(Direction::Rtl)
    );
}

#[test]
fn sizes_carry_their_unit() {
    let out = css(LayoutStyle::new()
        .width(SizeDimension::Px(300.0))
        .height(SizeDimension::Percent(0.5)));
    assert!(out.contains("width:300px;"), "got {out}");
    assert!(out.contains("height:50%;"), "got {out}");
}

#[test]
fn equal_edges_collapse_to_one_value() {
    let out = css(LayoutStyle::new().padding_all(24.0));
    assert!(out.contains("padding:24px;"), "got {out}");
}

#[test]
fn a_vertical_and_horizontal_pair_collapses_to_two() {
    let out = css(LayoutStyle::new()
        .padding_vertical(8.0)
        .padding_horizontal(16.0));
    assert!(out.contains("padding:8px 16px;"), "got {out}");
}

#[test]
fn four_different_edges_stay_four() {
    let out = css(LayoutStyle::new()
        .padding_top(1.0)
        .padding_right(2.0)
        .padding_bottom(3.0)
        .padding_left(4.0));
    assert!(out.contains("padding:1px 2px 3px 4px;"), "got {out}");
}

#[test]
fn zero_padding_is_left_unsaid() {
    assert!(
        !css(LayoutStyle::new().padding_all(0.0)).contains("padding"),
        "a zero padding is the default, so it needs no declaration: {}",
        css(LayoutStyle::new().padding_all(0.0))
    );
}

#[test]
fn a_gap_is_row_then_column() {
    let out = css(LayoutStyle::new().flex_row().gap_y(4.0).gap_x(12.0));
    assert!(out.contains("gap:4px 12px;"), "got {out}");
}

#[test]
fn a_shrink_that_is_not_the_default_is_stated() {
    assert!(
        css(LayoutStyle::new().flex_shrink(0.0)).contains("flex-shrink:0;"),
        "{}",
        css(LayoutStyle::new().flex_shrink(0.0))
    );
    assert!(
        !css(LayoutStyle::new()).contains("flex-shrink"),
        "the default shrink needs no declaration: {}",
        css(LayoutStyle::new())
    );
}

#[test]
fn a_grown_item_says_so() {
    assert!(
        css(LayoutStyle::new().flex_grow(1.0)).contains("flex-grow:1;"),
        "{}",
        css(LayoutStyle::new().flex_grow(1.0))
    );
}

#[test]
fn a_hidden_box_says_nothing_else() {
    assert_eq!(
        css(LayoutStyle::new()
            .flex_row()
            .padding_all(8.0)
            .display_none()),
        "display:none;"
    );
}

#[test]
fn an_absolute_fill_pins_every_edge() {
    let out = css(LayoutStyle::new().absolute_fill());
    assert!(out.contains("position:absolute;"), "got {out}");
    for edge in ["top:0px", "right:0px", "bottom:0px", "left:0px"] {
        assert!(out.contains(edge), "{edge} missing from {out}");
    }
}

#[test]
fn an_unpinned_absolute_box_leaves_its_edges_alone() {
    let out = css(LayoutStyle::new().absolute());
    assert!(out.contains("position:absolute;"), "got {out}");
    assert!(
        !out.contains("top:"),
        "an auto inset is not a declaration: {out}"
    );
}

#[test]
fn alignment_keeps_its_css_spelling() {
    let out = css(LayoutStyle::new()
        .flex_row()
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::SPACE_BETWEEN));
    assert!(out.contains("align-items:center;"), "got {out}");
    assert!(out.contains("justify-content:space-between;"), "got {out}");
}

#[test]
fn an_aspect_ratio_needs_no_unit() {
    assert!(
        css(LayoutStyle::new().aspect_ratio(1.5)).contains("aspect-ratio:1.5;"),
        "{}",
        css(LayoutStyle::new().aspect_ratio(1.5))
    );
}

/// A logical edge has to land on the physical one the direction chose, and land on the *same* one Taffy resolved it to — the whole point of going through `resolve` rather than reading the logical fields.
#[test]
fn a_logical_edge_follows_the_direction() {
    let style = LayoutStyle::new().padding_start(SizeDimension::Px(20.0));
    assert!(
        style
            .to_css(Direction::Ltr)
            .as_str()
            .contains("padding:0 0 0 20px;"),
        "ltr: {}",
        style.to_css(Direction::Ltr)
    );
    assert!(
        style
            .to_css(Direction::Rtl)
            .as_str()
            .contains("padding:0 20px 0 0;"),
        "rtl: {}",
        style.to_css(Direction::Rtl)
    );
}
