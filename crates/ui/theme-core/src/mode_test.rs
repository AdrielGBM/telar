use super::*;
use std::cell::Cell;

fn reset() {
    ACTIVE_MODE.with(|s| s.set(None));
    MODES.with(|m| m.borrow_mut().clear());
    SCHEME_PAIR.with(|p| *p.borrow_mut() = None);
    // Drop any prior follow_system effect first, so it stops reacting to SYSTEM_DARK in later tests.
    FOLLOW.with(|f| *f.borrow_mut() = None);
    SYSTEM_DARK.with(|s| s.set(false));
}

#[test]
fn set_mode_runs_apply_and_publishes_id() {
    reset();
    let hits = Rc::new(Cell::new(0));
    let h = hits.clone();
    register_mode("dark", move || h.set(h.get() + 1));
    set_mode("dark");
    assert_eq!(hits.get(), 1, "apply closure ran once");
    assert_eq!(active_mode().as_deref(), Some("dark"));
}

#[test]
fn set_mode_publishes_even_without_registration() {
    reset();
    set_mode("unregistered");
    assert_eq!(active_mode().as_deref(), Some("unregistered"));
}

#[test]
fn follow_system_drives_mode_from_os_scheme() {
    reset();
    register_mode("day", || {});
    register_mode("night", || {});
    follow_system("day", "night");
    assert_eq!(
        active_mode().as_deref(),
        Some("day"),
        "effect runs once with default SYSTEM_DARK=false → light"
    );
    set_system_dark(true);
    assert_eq!(active_mode().as_deref(), Some("night"));
    set_system_dark(false);
    assert_eq!(active_mode().as_deref(), Some("day"));
}

#[test]
fn is_dark_false_for_unpaired_third_mode() {
    reset();
    set_light_dark("day", "night");
    set_mode("pastel");
    assert!(
        !is_dark(),
        "a third mode outside the pair is neither dark nor light"
    );
}

#[test]
fn use_mode_is_reactive() {
    reset();
    let seen = Rc::new(RefCell::new(Vec::<Option<String>>::new()));
    let s = seen.clone();
    let _e = reactive_core::effect(move || s.borrow_mut().push(use_mode()));
    set_mode("a");
    set_mode("b");
    let got = seen.borrow().clone();
    assert_eq!(
        got,
        vec![None, Some("a".into()), Some("b".into())],
        "effect re-ran on each mode switch"
    );
}
