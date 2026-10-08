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

#[test]
fn a_surface_override_shadows_the_thread_direction_until_cleared() {
    set_direction(Direction::Ltr);
    let surface = DirectionContext::new();
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let _e = {
        let _entered = surface.enter();
        let s = seen.clone();
        reactive_core::effect(move || s.borrow_mut().push(use_direction()))
    };
    {
        let _entered = surface.enter();
        set_surface_direction(Some(Direction::Rtl));
        assert_eq!(current_direction(), Direction::Rtl);
        assert_eq!(use_surface_direction(), Some(Direction::Rtl));
    }
    assert_eq!(
        current_direction(),
        Direction::Ltr,
        "the ambient world keeps the thread's direction"
    );

    set_direction(Direction::Rtl);
    set_direction(Direction::Ltr);
    assert_eq!(
        *seen.borrow(),
        vec![Direction::Ltr, Direction::Rtl],
        "an overridden reader does not hear the thread's direction move"
    );

    {
        let _entered = surface.enter();
        set_surface_direction(None);
        assert_eq!(current_direction(), Direction::Ltr);
    }
    set_direction(Direction::Rtl);
    assert_eq!(
        *seen.borrow(),
        vec![
            Direction::Ltr,
            Direction::Rtl,
            Direction::Ltr,
            Direction::Rtl
        ],
        "once cleared, the surface follows the thread again"
    );
    set_direction(Direction::Ltr);
}
