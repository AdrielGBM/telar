use std::cell::Cell;
use std::rc::Rc;

use super::*;

fn counting() -> (Rc<Cell<u32>>, impl Fn() + Clone + 'static) {
    let count = Rc::new(Cell::new(0));
    let bump = {
        let count = Rc::clone(&count);
        move || count.set(count.get() + 1)
    };
    (count, bump)
}

#[test]
fn a_clone_runs_the_same_callback() {
    let (count, bump) = counting();
    let handler = Handler::from(bump);
    let copy = handler.clone();

    handler.run();
    copy.run();

    assert_eq!(count.get(), 2);
}

#[test]
fn a_shared_closure_becomes_a_handler() {
    let (count, bump) = counting();

    Handler::from(Rc::new(bump)).run();

    assert_eq!(count.get(), 1);
}

#[test]
fn a_closure_a_handler_and_a_given_option_each_press() {
    let (count, bump) = counting();

    let pressed = [
        bump.clone().into_press(),
        Handler::from(bump.clone()).into_press(),
        Some(Handler::from(bump)).into_press(),
    ];
    for handler in pressed.iter().flatten() {
        handler.run();
    }

    assert_eq!(count.get(), 3);
}

#[test]
fn an_option_left_unset_presses_nothing() {
    assert!(None::<Handler>.into_press().is_none());
}
