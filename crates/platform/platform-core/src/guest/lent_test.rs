use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::*;
use crate::guest::{CWaker, CallOutcome, guest_entry};
use crate::{Event, RedrawWaker};

#[test]
fn lent_bytes_read_back_as_the_slice_they_lend() {
    let bytes = [1u8, 2, 3];
    let lent = LentBytes::lend(&bytes);
    assert_eq!(lent.len(), 3);
    assert_eq!(unsafe { lent.as_slice() }, &bytes);
    assert!(LentBytes::EMPTY.is_empty());
    assert_eq!(unsafe { LentBytes::default().as_slice() }, &[] as &[u8]);
}

#[test]
fn an_answer_lends_the_latest_value_spelled_on_this_thread() {
    let first = LentBytes::answer(&String::from("first"));
    assert_eq!(unsafe { first.read::<String>() }, "first");
    let second = LentBytes::answer(&Some(7u64));
    assert_eq!(unsafe { second.read::<Option<u64>>() }, Some(7));
}

#[test]
fn bytes_an_entry_cannot_read_are_its_panic() {
    let spelled = wire_core::encode(&String::from("not a number"));
    let outcome = guest_entry(|| {
        unsafe { LentBytes::lend(&spelled).read::<Option<u64>>() };
    });
    let panic = unsafe { outcome.into_result() }.expect_err("the entry panics");
    assert!(
        panic
            .message
            .starts_with("the host lent bytes this guest cannot read as core::option::Option<u64>"),
        "{}",
        panic.message
    );
}

struct Counted(Rc<Cell<u32>>);

impl Drop for Counted {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

#[test]
fn a_handle_reaches_its_instance_until_it_is_released_once() {
    let drops = Rc::new(Cell::new(0));
    let handle = Handle::new((Counted(Rc::clone(&drops)), 5u32));
    assert!(!handle.is_null());
    unsafe { handle.get().1 += 1 };
    assert_eq!(unsafe { handle.get().1 }, 6);
    assert_eq!(drops.get(), 0);
    unsafe { handle.release() };
    assert_eq!(drops.get(), 1);
    assert!(Handle::<u32>::NULL.is_null());
}

unsafe extern "C" fn entry(
    handle: Handle<u32>,
    event: LentBytes,
    waker: CWaker,
    out: *mut LentBytes,
) -> CallOutcome {
    guest_entry(|| {
        let event: Event = unsafe { event.read() };
        let count = unsafe { handle.get() };
        *count += 1;
        waker.wake();
        unsafe { out.write(LentBytes::answer(&(event, *count))) };
    })
}

#[test]
fn an_entry_takes_and_lends_only_building_blocks_and_bytes() {
    let woken = Arc::new(AtomicBool::new(false));
    let seen = Arc::clone(&woken);
    let waker = CWaker::new(RedrawWaker::new(move || {
        seen.store(true, Ordering::Relaxed)
    }));
    let handle = Handle::new(41u32);
    let event = wire_core::encode(&Event::CursorEntered);
    let mut out = LentBytes::EMPTY;
    let outcome = unsafe { entry(handle, LentBytes::lend(&event), waker, &mut out) };
    assert_eq!(unsafe { outcome.into_result() }, Ok(()));
    assert_eq!(
        wire_core::decode::<(Event, u32)>(unsafe { out.as_slice() }),
        Ok((Event::CursorEntered, 42))
    );
    assert!(woken.load(Ordering::Relaxed));
    unsafe { handle.release() };
}
