use super::*;
use reactive_core::signal;
use services_core::Scope;

/// What a remount is, from the state's point of view: the tree is gone, the surface is not, and the value it was reading is still the same one.
#[test]
fn a_second_build_reads_back_what_the_first_one_kept() {
    Scope::with(|| {
        let first = kept("test.query", || signal(String::new()));
        first.set("telar".to_string());

        let second = kept("test.query", || signal(String::new()));
        assert_eq!(
            second.peek(),
            "telar",
            "the rebuilt tree subscribes to the signal the old one was writing, not to a fresh one"
        );
        assert_eq!(
            kept("test.other", || signal(String::new())).peek(),
            "",
            "and a different key is a different value"
        );
    });
}

/// Two surfaces are two stores, or the second window to open would inherit the first one's search box.
#[test]
fn one_surfaces_state_is_not_another_surfaces() {
    let first = Scope::with(|| kept("test.page", || signal(3usize)).peek());
    let second = Scope::with(|| kept("test.page", || signal(0usize)).peek());
    assert_eq!((first, second), (3, 0));
}
