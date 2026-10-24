use super::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[test]
fn a_call_that_returns_hands_back_what_it_returned() {
    assert_eq!(guest_call(|| 7), Ok(7));
}

/// What the user sees is what std would have printed: the message, and the line that panicked.
#[test]
fn a_panic_comes_back_with_its_message_and_where_it_happened() {
    let line = line!() + 2;
    let panic = guest_call(|| -> u32 {
        panic!("RUNTIME is already borrowed by {}", "an effect");
    })
    .expect_err("the panic is caught");
    assert_eq!(panic.message, "RUNTIME is already borrowed by an effect");
    let location = panic.location.as_deref().expect("the hook saw where");
    assert!(
        location.starts_with(&format!("{}:{line}:", file!())),
        "{location}"
    );
    assert_eq!(
        panic.to_string(),
        format!("panicked at {location}:\nRUNTIME is already borrowed by an effect")
    );
}

/// An unwind the hook never saw must not be reported under the location of an older panic.
#[test]
fn a_panic_the_hook_never_saw_is_not_given_an_older_location() {
    let _ = guest_call(|| panic!("first"));
    let panic = guest_call(|| std::panic::resume_unwind(Box::new("resumed")))
        .expect_err("the unwind is caught");
    assert_eq!(
        panic,
        GuestPanic {
            message: "resumed".to_owned(),
            location: None,
        }
    );
}

/// The guest's state is whatever the unwind left it in, so the host stops calling it and says why, once.
#[test]
fn a_guest_that_panicked_is_never_called_again() {
    let guest = Guest::default();
    assert_eq!(guest.call(|| Ok(1)), Some(1));
    assert_eq!(guest.take_panic(), None);

    let panicked = guest.call(|| guest_call(|| -> u32 { panic!("in a handler") }));
    assert_eq!(panicked, None);
    assert!(guest.has_panicked());

    let mut called = false;
    assert_eq!(
        guest.call(|| {
            called = true;
            Ok(2)
        }),
        None
    );
    assert!(!called, "nothing reaches a guest that panicked");
    let report = guest.take_panic().expect("the panic is reported");
    assert_eq!(report.message, "in a handler");
    assert_eq!(guest.take_panic(), None, "and reported once");
    assert_eq!(
        guest.panic().map(|panic| panic.message.as_str()),
        Some("in a handler"),
        "while what stopped it stays readable"
    );
}

/// Host code the guest calls into answers with the fallback rather than unwinding into the guest.
#[test]
fn host_code_that_panics_answers_with_the_fallback() {
    assert_eq!(
        shield(None, || -> Option<u32> { panic!("the loop is gone") }),
        None
    );
    assert_eq!(shield(None, || Some(3)), Some(3));

    let woken = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&woken);
    let waker = shielded_waker(RedrawWaker::new(move || {
        flag.store(true, Ordering::SeqCst);
        panic!("the loop is gone");
    }));
    waker.wake();
    assert!(woken.load(Ordering::SeqCst));
}

/// The guest's frame wakes the host's loop through a shielded waker and asks for a redraw on the host's context.
#[test]
fn a_guest_frame_carries_its_redraw_back_and_wakes_through_a_shield() {
    let woken = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&woken);
    let waker = RedrawWaker::new(move || {
        flag.store(true, Ordering::SeqCst);
        panic!("the loop is gone");
    });
    let mut redraw_requested = false;
    let mut ctx = AppCtx::new(&mut redraw_requested, Some(&waker), None, None);

    with_guest_ctx(&mut ctx, |guest_ctx| {
        guest_ctx
            .redraw_waker()
            .expect("the guest gets a waker")
            .wake();
    });
    assert!(woken.load(Ordering::SeqCst));
    assert!(!redraw_requested, "nothing asked for one yet");

    let mut ctx = AppCtx::new(&mut redraw_requested, Some(&waker), None, None);
    with_guest_ctx(&mut ctx, |guest_ctx| guest_ctx.request_redraw());
    assert!(redraw_requested);
}

/// Calls on several threads at once each get back their own panic, never one a neighbour raised.
#[test]
fn concurrent_calls_each_get_their_own_panic() {
    let handles: Vec<_> = (0..8)
        .map(|thread| {
            std::thread::spawn(move || {
                for round in 0..50 {
                    let message = format!("thread {thread} round {round}");
                    let panic = guest_call(|| -> u32 { panic!("{}", message.clone()) })
                        .expect_err("the panic is caught");
                    assert_eq!(panic.message, message);
                    assert!(panic.location.is_some());
                }
            })
        })
        .collect();
    for handle in handles {
        handle.join().expect("no thread saw another's panic");
    }
}

/// A guest call that makes another, as host code the guest was handed calling back in, keeps each panic with its own call.
#[test]
fn a_panic_in_a_nested_call_is_not_taken_by_the_outer_one() {
    let outer = guest_call(|| {
        let inner = guest_call(|| -> u32 { panic!("inner") }).expect_err("inner is caught");
        assert_eq!(inner.message, "inner");
        panic!("outer");
    })
    .expect_err("outer is caught");
    assert_eq!(outer.message, "outer");
}

#[test]
fn a_matching_layout_is_accepted_and_a_different_one_refused_with_what_to_do() {
    let host = crate::boundary_layout!(u32, (u8, u64); "shaper" => true);
    assert_eq!(host.accept(host), Ok(()));

    let other_feature = crate::boundary_layout!(u32, (u8, u64); "shaper" => false);
    let other_type = crate::boundary_layout!(u32, (u8, u8); "shaper" => true);
    for guest in [other_feature, other_type] {
        let refused = host
            .accept(guest)
            .expect_err("a different layout is refused");
        assert_eq!(refused, LayoutMismatch { host, guest });
        let message = refused.to_string();
        assert!(message.contains("restart the session"), "{message}");
        assert!(message.contains("matching features"), "{message}");
    }
}

type Calls = std::rc::Rc<std::cell::RefCell<Vec<&'static str>>>;

struct Mapping(Calls);

impl Drop for Mapping {
    fn drop(&mut self) {
        self.0.borrow_mut().push("unmapped");
    }
}

fn unload(guest: &Guest, quiesced: GuestResult<()>) -> Vec<&'static str> {
    let calls = Calls::default();
    guest.unload(
        Mapping(calls.clone()),
        || {
            calls.borrow_mut().push("unloading");
            Ok(())
        },
        || {
            calls.borrow_mut().push("quiesce");
            quiesced
        },
    );
    calls.take()
}

#[test]
fn a_guest_is_told_it_is_unloading_then_quiesced_then_unmapped() {
    assert_eq!(
        unload(&Guest::default(), Ok(())),
        ["unloading", "quiesce", "unmapped"]
    );
}

#[test]
fn a_guest_that_panicked_is_only_quiesced_before_it_is_unmapped() {
    let guest = Guest::default();
    guest.call(|| guest_call(|| panic!("a press handler panicked")));
    assert_eq!(unload(&guest, Ok(())), ["quiesce", "unmapped"]);
}

#[test]
fn a_guest_that_cannot_be_quiesced_stays_mapped() {
    let stuck = guest_call(|| panic!("a job would not stop"));
    assert_eq!(
        unload(&Guest::default(), stuck),
        ["unloading", "quiesce"],
        "a thread may still run its code"
    );
}

#[cfg(all(feature = "dylib", not(target_arch = "wasm32")))]
mod unique_copies {
    use super::super::*;
    use std::path::Path;

    #[test]
    fn every_copy_has_a_path_of_its_own_beside_the_original_with_its_extension() {
        let path = Path::new("/build/libapp.so");
        let first = unique_copy_path(path);
        let second = unique_copy_path(path);
        assert_ne!(first, second);
        for copy in [first, second] {
            assert_eq!(copy.parent(), path.parent());
            assert_eq!(copy.extension(), path.extension());
            assert_ne!(copy, path);
        }
    }

    #[test]
    fn a_library_that_fails_to_open_leaves_no_copy_behind() {
        let dir = std::env::temp_dir().join(format!("guest-open-unique-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create dir");
        let path = dir.join("not-a-library.so");
        std::fs::write(&path, b"not a library").expect("write file");

        assert!(open_unique(&path).is_err());

        let left: Vec<_> = std::fs::read_dir(&dir)
            .expect("read dir")
            .map(|entry| entry.expect("entry").path())
            .collect();
        std::fs::remove_dir_all(&dir).expect("clean up");
        assert_eq!(left, vec![path]);
    }
}
