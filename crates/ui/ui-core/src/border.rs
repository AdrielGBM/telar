//! The two box properties whose edges can be named by role rather than by side, resolved against the active writing direction.
//!
//! `start`/`end` mean here what `padding_start` means in layout: the edge the text comes from, and the one it runs towards. Layout resolves its own in a pass it already had; paint has no such pass, so the flip happens inside the style closure the renderer re-runs — which is also what makes a live LTR/RTL switch repaint instead of needing the tree rebuilt.

use renderer_core::BorderRadius;

use crate::context::use_direction;

/// Per-side border widths where `start`/`end`, when given, land on left or right according to the writing direction.
///
/// A logical side overrides the physical one it lands on: an author who wrote both named this edge twice, and the name that describes its *role* is the one that was talking about the current layout.
pub fn logical_border_widths(
    top: f32,
    right: f32,
    bottom: f32,
    left: f32,
    start: Option<f32>,
    end: Option<f32>,
) -> [f32; 4] {
    let (mut left, mut right) = (left, right);
    let rtl = use_direction().is_rtl();
    if let Some(w) = start {
        if rtl { right = w } else { left = w }
    }
    if let Some(w) = end {
        if rtl { left = w } else { right = w }
    }
    [top, right, bottom, left]
}

/// Corner radii where `start`/`end`, when given, round the two corners on that edge.
///
/// A side rather than a corner, because that is the shape the property is actually for: a panel tucked against the rail is rounded on the two corners facing away from it, and under RTL it has to tuck against the other rail without the author writing the layout twice.
pub fn logical_border_radius(
    top_left: f32,
    top_right: f32,
    bottom_right: f32,
    bottom_left: f32,
    start: Option<f32>,
    end: Option<f32>,
) -> BorderRadius {
    let mut r = BorderRadius {
        top_left,
        top_right,
        bottom_right,
        bottom_left,
    };
    let rtl = use_direction().is_rtl();
    if let Some(v) = start {
        if rtl {
            (r.top_right, r.bottom_right) = (v, v);
        } else {
            (r.top_left, r.bottom_left) = (v, v);
        }
    }
    if let Some(v) = end {
        if rtl {
            (r.top_left, r.bottom_left) = (v, v);
        } else {
            (r.top_right, r.bottom_right) = (v, v);
        }
    }
    r
}

#[cfg(test)]
mod tests {
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
}
