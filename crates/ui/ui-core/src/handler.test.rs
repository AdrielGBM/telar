use std::cell::{Cell, RefCell};
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
fn a_handler_is_called_with_its_argument() {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let handler: Handler<fn(bool)> = {
        let seen = Rc::clone(&seen);
        Handler::from(move |on| seen.borrow_mut().push(on))
    };

    handler.run(true);
    handler.clone().run(false);

    assert_eq!(*seen.borrow(), [true, false]);
}

#[test]
fn a_handler_is_called_with_each_of_its_arguments() {
    let seen = Rc::new(Cell::new((0.0, 0.0)));
    let handler: Handler<fn(f32, f32)> = {
        let seen = Rc::clone(&seen);
        Handler::from(move |x, y| seen.set((x, y)))
    };

    handler.run(3.0, 4.0);

    assert_eq!(seen.get(), (3.0, 4.0));
}

#[test]
fn a_closure_a_handler_and_a_given_option_each_hand_over_their_callback() {
    let (count, bump) = counting();

    let pressed: [Option<Handler>; 3] = [
        bump.clone().into_handler(),
        Handler::from(bump.clone()).into_handler(),
        Some(Handler::from(bump)).into_handler(),
    ];
    for handler in pressed.iter().flatten() {
        handler.run();
    }

    assert_eq!(count.get(), 3);
}

#[test]
fn an_option_left_unset_hands_over_nothing() {
    assert!(None::<Handler<fn(bool)>>.into_handler().is_none());
}

#[test]
fn a_closure_with_arguments_hands_over_a_handler_of_them() {
    let seen = Rc::new(Cell::new(None));
    let forwarded = {
        let seen = Rc::clone(&seen);
        move |hovered: bool| seen.set(Some(hovered))
    };

    let handler: Option<Handler<fn(bool)>> = forwarded.into_handler();
    handler.expect("a closure is always a handler").run(true);

    assert_eq!(seen.get(), Some(true));
}
