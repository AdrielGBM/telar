use super::*;
use reactive_core::signal;
use services_core::Scope;

use crate::Surface;
use crate::disposal::OwnedBuild;

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

/// Regression: the store was provided on the owner of the build that first asked, so dropping that build took the store and the signal it held with it, and the next build silently started over.
#[test]
fn a_dropped_build_takes_neither_the_store_nor_what_it_kept() {
    let surface = Surface::new();
    let _entered = surface.enter();

    let (first, build) = OwnedBuild::run(|| kept("test.offset", || signal(0.0f32)));
    first.set(120.0);
    drop(build);
    assert!(
        first.is_alive(),
        "the kept signal outlives the build that made it"
    );

    let (second, _build) = OwnedBuild::run(|| kept("test.offset", || signal(0.0f32)));
    assert_eq!(
        second.peek(),
        120.0,
        "the rebuilt tree reads the offset the dropped one left"
    );
}

/// Two surfaces are two stores, or the second window to open would inherit the first one's search box.
#[test]
fn one_surfaces_state_is_not_another_surfaces() {
    let first = Surface::new();
    let second = Surface::new();
    let read = |surface: &Surface, init: usize| {
        let _entered = surface.enter();
        Scope::with(|| kept("test.page", || signal(init)).peek())
    };
    assert_eq!((read(&first, 3), read(&second, 0)), (3, 0));
    assert_eq!(read(&first, 0), 3, "and each keeps its own across builds");
}

/// A closed surface forgets what it kept: its world is disposed with it, and the signals in its store with that.
#[test]
fn a_closed_surface_frees_what_it_kept() {
    let surface = Surface::new();
    let value = {
        let _entered = surface.enter();
        Scope::with(|| kept("test.query", || signal(String::new())))
    };
    drop(surface);
    assert!(!value.is_alive());
}
