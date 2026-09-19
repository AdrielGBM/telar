use std::rc::Rc;

use super::*;

#[test]
fn a_run_asked_for_mid_run_is_applied_after_it_with_the_newest_value() {
    let serial = Rc::new(Serial::new());
    let seen = Rc::new(RefCell::new(Vec::new()));
    let (inner, log) = (Rc::clone(&serial), Rc::clone(&seen));
    serial.run(1, |v| {
        log.borrow_mut().push(v);
        if v == 1 {
            inner.run(2, |_| panic!("must not run inside the first pass"));
            inner.run(3, |_| panic!("must not run inside the first pass"));
        }
    });
    assert_eq!(*seen.borrow(), vec![1, 3]);
}

#[test]
fn a_panicking_run_leaves_the_next_one_free_to_start() {
    let serial = Serial::new();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        serial.run(1, |_| panic!("build failed"));
    }));
    assert!(outcome.is_err());
    let mut ran = Vec::new();
    serial.run(2, |v| ran.push(v));
    assert_eq!(ran, vec![2]);
}
