use super::*;

#[test]
fn sides_pick_the_shorter_and_the_longer() {
    let portrait = Size::new(390.0, 844.0);
    assert_eq!(portrait.min_side(), 390.0);
    assert_eq!(portrait.max_side(), 844.0);
}

#[test]
fn zero_is_the_default() {
    assert_eq!(Size::default(), Size::ZERO);
}
