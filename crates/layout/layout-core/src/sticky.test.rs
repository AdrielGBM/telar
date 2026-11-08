use super::*;

const NO_MARGINS: Margins = taffy::Rect {
    left: 0.0,
    right: 0.0,
    top: 0.0,
    bottom: 0.0,
};

fn top(px: f32) -> StickyInsets {
    StickyInsets {
        top: Some(SizeDimension::Px(px)),
        ..StickyInsets::default()
    }
}

fn bottom(px: f32) -> StickyInsets {
    StickyInsets {
        bottom: Some(SizeDimension::Px(px)),
        ..StickyInsets::default()
    }
}

/// A 400px-tall view over a 2000px-tall container, with a 50px box laid out 300px down.
fn shift(insets: StickyInsets, scrolled: f32) -> (f32, f32) {
    displacement(
        &insets,
        Rect::new(0.0, 300.0, 100.0, 50.0),
        NO_MARGINS,
        Rect::new(0.0, 0.0, 100.0, 2000.0),
        Rect::new(0.0, scrolled, 100.0, 400.0),
    )
}

#[test]
fn a_box_short_of_its_edge_stays_where_layout_put_it() {
    assert_eq!(shift(top(10.0), 0.0), (0.0, 0.0));
    assert_eq!(shift(top(10.0), 290.0), (0.0, 0.0));
}

#[test]
fn a_box_scrolled_past_its_edge_keeps_its_distance_from_it() {
    assert_eq!(shift(top(10.0), 500.0), (0.0, 210.0));
}

#[test]
fn a_box_never_leaves_its_containing_block() {
    let (_, dy) = shift(top(0.0), 5000.0);
    assert_eq!(
        dy,
        2000.0 - 50.0 - 300.0,
        "it stops with its bottom on the block's"
    );
}

#[test]
fn the_margin_is_kept_inside_the_containing_block() {
    let (_, dy) = displacement(
        &top(0.0),
        Rect::new(0.0, 300.0, 100.0, 50.0),
        taffy::Rect {
            bottom: 20.0,
            ..NO_MARGINS
        },
        Rect::new(0.0, 0.0, 100.0, 1000.0),
        Rect::new(0.0, 5000.0, 100.0, 400.0),
    );
    assert_eq!(dy, 1000.0 - 20.0 - 50.0 - 300.0);
}

#[test]
fn a_bottom_inset_lifts_a_box_below_the_view() {
    let lifted = |scrolled: f32| {
        displacement(
            &bottom(10.0),
            Rect::new(0.0, 700.0, 100.0, 50.0),
            NO_MARGINS,
            Rect::new(0.0, 0.0, 100.0, 2000.0),
            Rect::new(0.0, scrolled, 100.0, 400.0),
        )
    };
    assert_eq!(lifted(0.0), (0.0, 400.0 - 10.0 - 50.0 - 700.0));
    assert_eq!(lifted(400.0), (0.0, 0.0), "once in view it rests");
}

#[test]
fn a_bottom_inset_stops_at_the_top_of_its_containing_block() {
    let (_, dy) = displacement(
        &bottom(0.0),
        Rect::new(0.0, 900.0, 100.0, 50.0),
        NO_MARGINS,
        Rect::new(0.0, 800.0, 100.0, 200.0),
        Rect::new(0.0, 0.0, 100.0, 400.0),
    );
    assert_eq!(dy, -100.0);
}

/// A percentage is of the view, as CSS resolves a sticky inset against the scrollport.
#[test]
fn a_percentage_is_a_fraction_of_the_view() {
    let insets = StickyInsets {
        top: Some(SizeDimension::Percent(0.25)),
        ..StickyInsets::default()
    };
    assert_eq!(shift(insets, 500.0), (0.0, 500.0 + 100.0 - 300.0));
}

#[test]
fn when_both_edges_cannot_hold_the_top_wins() {
    let both = StickyInsets {
        top: Some(SizeDimension::Px(0.0)),
        bottom: Some(SizeDimension::Px(0.0)),
        ..StickyInsets::default()
    };
    let (_, dy) = displacement(
        &both,
        Rect::new(0.0, 900.0, 100.0, 600.0),
        NO_MARGINS,
        Rect::new(0.0, 0.0, 100.0, 5000.0),
        Rect::new(0.0, 1000.0, 100.0, 400.0),
    );
    assert_eq!(dy, 100.0, "the top edge is held, the bottom one overflows");
}

#[test]
fn the_horizontal_axis_follows_the_same_rule() {
    let insets = StickyInsets {
        left: Some(SizeDimension::Px(8.0)),
        ..StickyInsets::default()
    };
    let (dx, dy) = displacement(
        &insets,
        Rect::new(300.0, 0.0, 50.0, 40.0),
        NO_MARGINS,
        Rect::new(0.0, 0.0, 2000.0, 40.0),
        Rect::new(600.0, 0.0, 400.0, 40.0),
    );
    assert_eq!((dx, dy), (308.0, 0.0));
}

#[test]
fn an_edge_left_auto_does_not_stick() {
    assert_eq!(
        shift(StickyInsets::default(), 1000.0),
        (0.0, 0.0),
        "a sticky box with no inset is an ordinary box"
    );
}
