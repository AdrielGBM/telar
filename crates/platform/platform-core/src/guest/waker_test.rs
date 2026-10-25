use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use super::*;

fn counting() -> (Arc<AtomicU32>, RedrawWaker) {
    let wakes = Arc::new(AtomicU32::new(0));
    let counted = Arc::clone(&wakes);
    let waker = RedrawWaker::new(move || {
        counted.fetch_add(1, Ordering::Relaxed);
    });
    (wakes, waker)
}

#[test]
fn every_reference_wakes_the_waker_it_was_made_from() {
    let (wakes, waker) = counting();
    let lent = CWaker::new(waker);
    lent.wake();
    let copy = lent.clone();
    copy.wake();
    let theirs = copy.into_redraw_waker();
    theirs.wake();
    theirs.clone().wake();
    assert_eq!(wakes.load(Ordering::Relaxed), 4);
}

#[test]
fn the_waker_is_freed_once_the_last_reference_is_released() {
    let (wakes, waker) = counting();
    let lent = CWaker::new(waker);
    let copy = lent.clone();
    assert_eq!(Arc::strong_count(&wakes), 2);
    drop(lent);
    assert_eq!(Arc::strong_count(&wakes), 2);
    drop(copy);
    assert_eq!(Arc::strong_count(&wakes), 1);
}

#[test]
fn a_waker_that_panics_stops_on_the_side_that_made_it() {
    let lent = CWaker::new(RedrawWaker::new(|| panic!("the loop is gone")));
    lent.wake();
}

#[test]
fn a_waker_can_be_woken_from_another_thread() {
    let (wakes, waker) = counting();
    let lent = CWaker::new(waker);
    std::thread::spawn(move || lent.wake()).join().unwrap();
    assert_eq!(wakes.load(Ordering::Relaxed), 1);
}
