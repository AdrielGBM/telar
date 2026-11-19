use layout_core::LayoutStyle;

use super::*;
use crate::context::{new_leaf, reset_layout_runtime};

/// Three 40px rows stacked from the top of a 200px tall area.
fn rows() -> AreaLayout {
    AreaLayout {
        bounds: Rect::new(0.0, 0.0, 100.0, 200.0),
        items: (0..3)
            .map(|row| Rect::new(0.0, row as f32 * 40.0, 100.0, 40.0))
            .collect(),
    }
}

/// A vertical area over a box of its own, accepting what `accepts` says.
fn area(accepts: impl Fn(&(), DropPlace<usize>) -> bool + 'static) -> DropArea<()> {
    reset_layout_runtime();
    let (node, _) = new_leaf(LayoutStyle::new()).unwrap();
    DropArea::new(node, Axis::Vertical, accepts, |_, _| {})
}

fn at(area: &DropArea<()>, y: f32) -> Option<DropPlace<usize>> {
    place_at(area, &(), &rows(), (50.0, y))
}

#[test]
fn over_an_item_that_takes_the_payload_its_middle_is_inside_and_its_edges_the_gaps() {
    let area = area(|_, _| true);
    assert_eq!(at(&area, 42.0), Some(DropPlace::Between(1)));
    assert_eq!(at(&area, 60.0), Some(DropPlace::Inside(1)));
    assert_eq!(at(&area, 78.0), Some(DropPlace::Between(2)));
}

#[test]
fn over_an_item_that_refuses_it_each_half_is_the_gap_on_its_side() {
    let area = area(|_, place| matches!(place, DropPlace::Between(_)));
    assert_eq!(at(&area, 55.0), Some(DropPlace::Between(1)));
    assert_eq!(at(&area, 65.0), Some(DropPlace::Between(2)));
}

#[test]
fn a_refused_gap_gives_way_to_the_item_beside_it_that_takes_the_payload() {
    let area = area(|_, place| matches!(place, DropPlace::Inside(_)));
    assert_eq!(at(&area, 42.0), Some(DropPlace::Inside(1)));
    assert_eq!(
        at(&area, 150.0),
        None,
        "past the items there is only the gap, and it is refused"
    );
}

#[test]
fn past_the_items_a_payload_lands_after_the_last() {
    let area = area(|_, _| true);
    assert_eq!(at(&area, 150.0), Some(DropPlace::Between(3)));
}

#[test]
fn an_area_with_no_items_takes_a_payload_into_its_one_gap() {
    let area = area(|_, _| true);
    let empty = AreaLayout {
        bounds: Rect::new(0.0, 0.0, 100.0, 40.0),
        items: Vec::new(),
    };
    assert_eq!(
        place_at(&area, &(), &empty, (50.0, 20.0)),
        Some(DropPlace::Between(0))
    );
}

#[test]
fn a_scroll_area_scrolls_faster_the_nearer_the_pointer_is_to_its_edge() {
    let far = edge_speed(190.0, 10.0, 200.0, 0.0, 300.0);
    let near = edge_speed(198.0, 2.0, 200.0, 0.0, 300.0);
    assert!(near > far && far > 0.0, "{near} {far}");
    assert!(
        edge_speed(2.0, 198.0, 200.0, 50.0, 300.0) < 0.0,
        "back up near the top"
    );
    assert_eq!(
        edge_speed(100.0, 100.0, 200.0, 50.0, 300.0),
        0.0,
        "away from both edges"
    );
}

#[test]
fn a_scroll_area_already_at_the_end_it_is_held_against_stays_put() {
    assert_eq!(edge_speed(198.0, 2.0, 200.0, 300.0, 300.0), 0.0);
    assert_eq!(edge_speed(2.0, 198.0, 200.0, 0.0, 300.0), 0.0);
}

#[test]
fn a_place_counted_one_way_is_the_same_place_counted_another() {
    assert_eq!(
        DropPlace::Inside(2).map(|index| (7, index)),
        DropPlace::Inside((7, 2))
    );
    assert_eq!(*DropPlace::Between(4).at(), 4);
}
