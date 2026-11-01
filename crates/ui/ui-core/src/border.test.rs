use layout_core::Direction;

use super::*;
use crate::context::set_direction;

#[test]
fn a_logical_side_lands_on_the_edge_the_text_comes_from() {
    set_direction(Direction::Ltr);
    assert_eq!(
        logical_border_widths(0.0, 0.0, 0.0, 0.0, Some(2.0), None),
        [0.0, 0.0, 0.0, 2.0]
    );
    set_direction(Direction::Rtl);
    assert_eq!(
        logical_border_widths(0.0, 0.0, 0.0, 0.0, Some(2.0), None),
        [0.0, 2.0, 0.0, 0.0]
    );
    set_direction(Direction::Ltr);
}

#[test]
fn a_physical_side_stays_where_it_was_put() {
    for direction in [Direction::Ltr, Direction::Rtl] {
        set_direction(direction);
        assert_eq!(
            logical_border_widths(0.0, 0.0, 0.0, 1.0, None, None),
            [0.0, 0.0, 0.0, 1.0],
            "{direction:?}"
        );
    }
    set_direction(Direction::Ltr);
}

/// Both names for one edge: the role wins, since that is the one that knew which layout it was in.
#[test]
fn a_logical_side_overrides_the_physical_one_it_lands_on() {
    set_direction(Direction::Ltr);
    assert_eq!(
        logical_border_widths(0.0, 0.0, 0.0, 1.0, Some(2.0), None),
        [0.0, 0.0, 0.0, 2.0]
    );
    set_direction(Direction::Rtl);
    assert_eq!(
        logical_border_widths(0.0, 0.0, 0.0, 1.0, Some(2.0), None),
        [0.0, 2.0, 0.0, 1.0],
        "the left keeps what it was given: start went to the other edge"
    );
    set_direction(Direction::Ltr);
}

#[test]
fn a_logical_radius_rounds_both_corners_of_its_edge() {
    set_direction(Direction::Ltr);
    assert_eq!(
        logical_border_radius(0.0, 0.0, 0.0, 0.0, Some(8.0), None),
        BorderRadius {
            top_left: 8.0,
            top_right: 0.0,
            bottom_right: 0.0,
            bottom_left: 8.0,
        }
    );
    set_direction(Direction::Rtl);
    assert_eq!(
        logical_border_radius(0.0, 0.0, 0.0, 0.0, Some(8.0), None),
        BorderRadius {
            top_left: 0.0,
            top_right: 8.0,
            bottom_right: 8.0,
            bottom_left: 0.0,
        }
    );
    set_direction(Direction::Ltr);
}
