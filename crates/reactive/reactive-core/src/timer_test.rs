use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::*;
use crate::runtime::{SurfaceEnterGuard, set_current_surface, set_surface_enter_hook};
use crate::{effect, signal};

const DELAY: Duration = Duration::from_millis(500);

fn counter() -> (Rc<Cell<u32>>, impl FnOnce() + 'static) {
    let fired = Rc::new(Cell::new(0));
    let sink = fired.clone();
    (fired, move || sink.set(sink.get() + 1))
}

fn pass(by: Duration) {
    advance_timer_clock(by);
    fire_timers();
}

#[test]
fn it_runs_once_its_delay_has_passed_and_never_again() {
    let (fired, done) = counter();
    let timer = run_after(DELAY, done);

    pass(DELAY / 2);
    assert_eq!(fired.get(), 0);
    assert!(timer.is_pending());

    pass(DELAY);
    assert_eq!(fired.get(), 1);
    assert!(!timer.is_pending());

    pass(DELAY * 4);
    assert_eq!(fired.get(), 1);
}

#[test]
fn dropping_it_cancels_it() {
    let (fired, done) = counter();
    drop(run_after(DELAY, done));
    assert_eq!(pending_timer_count(), 0);
    pass(DELAY * 2);
    assert_eq!(fired.get(), 0);
}

#[test]
fn the_earliest_deadline_reads_the_same_until_it_changes() {
    assert_eq!(next_timer_due(), None);
    let (_, done) = counter();
    let later = run_after(DELAY * 2, done);
    let due = next_timer_due().expect("a deadline");

    advance_timer_clock(DELAY / 2);
    assert_eq!(
        next_timer_due(),
        Some(due),
        "time passing moves no deadline"
    );

    let (_, done) = counter();
    let sooner = run_after(DELAY, done);
    let earlier = next_timer_due().expect("a deadline");
    assert!(earlier < due, "a sooner timer is the new deadline");

    later.pause();
    drop(sooner);
    assert_eq!(next_timer_due(), None, "a paused timer has no deadline");
}

#[test]
fn the_loop_is_told_how_long_to_sleep() {
    assert_eq!(until_next_timer(), None, "nothing to wake for");
    let (_, done) = counter();
    let later = run_after(DELAY * 2, done);
    let (_, done) = counter();
    let sooner = run_after(DELAY, done);

    let wait = until_next_timer().expect("a wait");
    assert!(wait <= DELAY && wait > DELAY / 2, "{wait:?}");

    advance_timer_clock(DELAY * 3);
    assert_eq!(until_next_timer(), Some(Duration::ZERO), "overdue");
    drop((later, sooner));
}

#[test]
fn a_paused_timer_keeps_what_was_left_and_wakes_nobody() {
    let (fired, done) = counter();
    let timer = run_after(DELAY, done);
    pass(DELAY / 2);
    timer.pause();
    assert!(timer.is_paused());

    pass(DELAY * 4);
    assert_eq!(fired.get(), 0, "paused, it does not come due");
    assert_eq!(until_next_timer(), None, "and the loop sleeps until woken");

    timer.resume();
    assert!(!timer.is_paused());
    pass(DELAY / 4);
    assert_eq!(fired.get(), 0, "half of the delay was still left");
    pass(DELAY / 2);
    assert_eq!(fired.get(), 1);
}

#[test]
fn due_timers_run_earliest_first() {
    let order = Rc::new(RefCell::new(Vec::new()));
    let push = |name: &'static str| {
        let order = order.clone();
        move || order.borrow_mut().push(name)
    };
    let _late = run_after(DELAY * 2, push("late"));
    let _early = run_after(DELAY, push("early"));
    pass(DELAY * 3);
    assert_eq!(*order.borrow(), ["early", "late"]);
}

#[test]
fn a_callback_may_cancel_one_that_is_also_due() {
    let (fired, done) = counter();
    let second: Rc<RefCell<Option<Timer>>> = Rc::new(RefCell::new(None));
    let _first = run_after(DELAY, {
        let second = second.clone();
        move || drop(second.borrow_mut().take())
    });
    *second.borrow_mut() = Some(run_after(DELAY * 2, done));
    pass(DELAY * 3);
    assert_eq!(fired.get(), 0);
    assert_eq!(pending_timer_count(), 0);
}

#[test]
fn a_callback_may_schedule_another() {
    let (fired, done) = counter();
    let next: Rc<RefCell<Option<Timer>>> = Rc::new(RefCell::new(None));
    let _first = run_after(DELAY, {
        let next = next.clone();
        move || *next.borrow_mut() = Some(run_after(Duration::ZERO, done))
    });
    pass(DELAY);
    assert_eq!(
        fired.get(),
        0,
        "scheduled during the pass, it waits for the next"
    );
    fire_timers();
    assert_eq!(fired.get(), 1);
}

#[test]
fn callbacks_run_in_one_batch() {
    let value = signal(0);
    let runs = Rc::new(Cell::new(0));
    let _effect = effect({
        let runs = runs.clone();
        move || {
            value.get();
            runs.set(runs.get() + 1);
        }
    });
    let before = runs.get();
    let _a = run_after(DELAY, move || value.set(1));
    let _b = run_after(DELAY, move || value.set(2));
    pass(DELAY);
    assert_eq!(runs.get(), before + 1);
    assert_eq!(value.peek(), 2);
}

#[test]
fn a_callback_runs_in_the_surface_it_was_scheduled_from() {
    let entered = Rc::new(Cell::new(SurfaceHandle::NONE));
    set_surface_enter_hook({
        let entered = entered.clone();
        move |surface| {
            entered.set(surface);
            let previous = set_current_surface(surface);
            SurfaceEnterGuard::new(move || {
                set_current_surface(previous);
            })
        }
    });
    let previous = set_current_surface(SurfaceHandle(7));
    let seen = Rc::new(Cell::new(SurfaceHandle::NONE));
    let _timer = run_after(DELAY, {
        let seen = seen.clone();
        move || seen.set(current_surface())
    });
    set_current_surface(previous);

    pass(DELAY);
    assert_eq!(seen.get(), SurfaceHandle(7));
    assert_eq!(entered.get(), SurfaceHandle(7));
}

#[test]
fn cancelling_a_surface_cancels_only_its_timers() {
    let (mine, done) = counter();
    let previous = set_current_surface(SurfaceHandle(3));
    let _mine = run_after(DELAY, done);
    set_current_surface(previous);
    let (theirs, done) = counter();
    let _theirs = run_after(DELAY, done);

    cancel_timers_for(SurfaceHandle(3));
    pass(DELAY);
    assert_eq!((mine.get(), theirs.get()), (0, 1));
}
