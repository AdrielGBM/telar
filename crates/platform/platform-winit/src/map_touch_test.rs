use super::*;

fn drag(touch: &mut TouchDrag, x: f64, y: f64) -> Option<(f32, f32)> {
    touch.advance(x, y, 1)
}

#[test]
fn a_finger_that_moves_covers_the_distance_between_its_reports() {
    let mut touch = TouchDrag::default();
    assert_eq!(
        drag(&mut touch, 100.0, 200.0),
        None,
        "nothing to measure yet"
    );
    assert_eq!(drag(&mut touch, 100.0, 180.0), Some((0.0, -20.0)));
    assert_eq!(drag(&mut touch, 90.0, 170.0), Some((-10.0, -10.0)));
}

#[test]
fn a_finger_lifting_leaves_nothing_behind_for_the_next_one() {
    let mut touch = TouchDrag::default();
    drag(&mut touch, 0.0, 0.0);
    drag(&mut touch, 0.0, 50.0);
    touch.end();
    // Without the reset the next gesture opens with the jump from wherever the last one ended, which on a long page is the whole list moving at once.
    assert_eq!(drag(&mut touch, 0.0, 400.0), None);
}

#[test]
fn a_second_finger_landing_is_not_a_distance_from_the_first() {
    let mut touch = TouchDrag::default();
    touch.advance(0.0, 0.0, 1);
    assert_eq!(touch.advance(300.0, 0.0, 2), None);
    assert_eq!(touch.advance(300.0, 40.0, 2), Some((0.0, 40.0)));
}
