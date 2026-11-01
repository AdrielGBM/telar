use super::*;

fn strip(count: usize) -> Vec<Rect> {
    (0..count)
        .map(|i| Rect {
            x: i as f32 * 100.0,
            y: 0.0,
            width: 100.0,
            height: 40.0,
        })
        .collect()
}

#[test]
fn a_slot_turns_at_an_items_centre_not_at_its_edge() {
    let rects = strip(3);
    assert_eq!(insertion_index(&rects, (49.0, 20.0), Axis::Horizontal), 0);
    assert_eq!(insertion_index(&rects, (51.0, 20.0), Axis::Horizontal), 1);
    assert_eq!(insertion_index(&rects, (151.0, 20.0), Axis::Horizontal), 2);
}

#[test]
fn past_the_last_item_is_the_slot_after_it() {
    let rects = strip(3);
    assert_eq!(insertion_index(&rects, (999.0, 20.0), Axis::Horizontal), 3);
    assert_eq!(insertion_index(&rects, (-999.0, 20.0), Axis::Horizontal), 0);
}

#[test]
fn a_vertical_strip_reads_the_other_coordinate() {
    let rects: Vec<Rect> = (0..3)
        .map(|i| Rect {
            x: 0.0,
            y: i as f32 * 40.0,
            width: 100.0,
            height: 40.0,
        })
        .collect();
    assert_eq!(insertion_index(&rects, (50.0, 21.0), Axis::Vertical), 1);
    assert_eq!(insertion_index(&rects, (50.0, 21.0), Axis::Horizontal), 0);
}

/// The off-by-one both hand-rolled versions had to solve: a slot counted before the item is lifted out.
#[test]
fn moving_rightwards_accounts_for_the_hole_left_behind() {
    let mut items = vec!['a', 'b', 'c', 'd'];
    assert!(
        apply_move(&mut items, 0, 3),
        "moving an item rightwards is a real move"
    );
    assert_eq!(items, vec!['b', 'c', 'a', 'd']);
}

#[test]
fn moving_leftwards_lands_on_the_slot_as_counted() {
    let mut items = vec!['a', 'b', 'c', 'd'];
    assert!(apply_move(&mut items, 3, 1), "and so is moving leftwards");
    assert_eq!(items, vec!['a', 'd', 'b', 'c']);
}

#[test]
fn dropping_where_it_already_is_moves_nothing() {
    let mut items = vec!['a', 'b', 'c'];
    assert!(
        !apply_move(&mut items, 1, 1),
        "dropping an item on its own slot moves nothing"
    );
    assert!(
        !apply_move(&mut items, 1, 2),
        "and neither does dropping it just past itself"
    );
    assert_eq!(items, vec!['a', 'b', 'c']);
}

#[test]
fn an_out_of_range_source_is_refused_rather_than_panicking() {
    let mut items = vec!['a'];
    assert!(
        !apply_move(&mut items, 5, 0),
        "an out-of-range source is refused rather than panicking"
    );
    assert_eq!(items, vec!['a']);
}
