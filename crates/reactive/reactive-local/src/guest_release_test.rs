//! What a copy of the runtime left to its host frees when the host unloads it, and what it leaves alone until then.

use std::sync::Mutex;

use telar_reactive_local::thread_exit::{
    leave_to_host, on_thread_exit, on_unload, release_guest, release_thread,
};

static RAN: Mutex<Vec<&str>> = Mutex::new(Vec::new());

fn ran(release: &'static str) {
    RAN.lock().unwrap().push(release);
}

fn static_release() {
    ran("static");
    on_thread_exit(slot_filled_by_a_static);
}

fn slot_release() {
    ran("slot");
    on_unload(static_filled_by_a_slot);
}

fn slot_filled_by_a_static() {
    ran("slot filled by a static");
}

fn static_filled_by_a_slot() {
    ran("static filled by a slot");
}

fn exiting_thread_release() {
    ran("exiting thread");
}

/// Each case is a step of one sequence: the copy is left to its host once, for the whole process.
#[test]
fn unloading_frees_the_statics_and_the_calling_thread_until_neither_holds_anything() {
    leave_to_host();

    std::thread::spawn(|| on_thread_exit(exiting_thread_release))
        .join()
        .unwrap();
    assert!(
        RAN.lock().unwrap().is_empty(),
        "a guest's thread registers no destructor that would pin its library, so it frees nothing as it exits"
    );

    on_unload(static_release);
    on_thread_exit(slot_release);
    release_guest();
    assert_eq!(
        *RAN.lock().unwrap(),
        [
            "static",
            "slot filled by a static",
            "slot",
            "static filled by a slot"
        ]
    );

    RAN.lock().unwrap().clear();
    release_guest();
    release_thread();
    assert!(RAN.lock().unwrap().is_empty(), "each release runs once");
}
