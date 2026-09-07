use super::*;

#[test]
fn direction_is_reactive_and_starts_left_to_right() {
    set_direction(Direction::Ltr);
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let s = seen.clone();
    let _e = reactive_core::effect(move || s.borrow_mut().push(use_direction()));
    set_direction(Direction::Rtl);
    set_direction(Direction::Rtl);
    assert_eq!(
        *seen.borrow(),
        vec![Direction::Ltr, Direction::Rtl],
        "the effect re-ran once, not twice: setting the same direction is not a change"
    );
    set_direction(Direction::Ltr);
}
