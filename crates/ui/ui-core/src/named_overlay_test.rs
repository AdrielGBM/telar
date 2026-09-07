use super::*;

fn reset() {
    NAMED.with(|named| named.borrow_mut().clear());
}

#[test]
fn a_name_resolves_to_one_shared_signal() {
    reset();
    let first = state("confirm");
    let second = state("confirm");
    first.set(true);
    assert!(second.get(), "both handles are the same signal");
    assert!(!state("other").get(), "a different name is independent");
}

/// The ordering that makes named overlays worth having: something opens the name before the widget for it exists (a restored deep link, a shortcut fired during startup), and the widget still comes up open.
#[test]
fn opening_a_name_before_its_overlay_is_built_still_opens_it() {
    reset();
    open("settings");
    assert!(
        state("settings").get(),
        "a name can be opened before anything is built for it"
    );
}

#[test]
fn open_and_close_drive_the_same_state() {
    reset();
    assert!(!state("panel").peek(), "the overlay starts closed");
    open("panel");
    assert!(state("panel").peek(), "open drives the state");
    close("panel");
    assert!(!state("panel").peek(), "and close drives the same one back");
}

/// The state is a real signal, so a trigger that styles itself as active while its panel is up re-runs on open and close without any wiring of its own.
#[test]
fn the_state_is_reactive() {
    reset();
    let seen = std::rc::Rc::new(RefCell::new(Vec::new()));
    let s = seen.clone();
    let _e = reactive_core::effect(move || s.borrow_mut().push(state("banner").get()));
    open("banner");
    close("banner");
    assert_eq!(*seen.borrow(), vec![false, true, false]);
}
