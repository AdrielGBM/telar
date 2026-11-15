use std::cell::RefCell;
use std::rc::Rc;

use geometry_core::Size;
use layout_core::LayoutStyle;

use super::*;
use crate::{Container, Surface, SurfaceCanvas, box_item};

const CONFIRM: OverlayName = OverlayName::new("confirm");
const OTHER: OverlayName = OverlayName::new("other");
const SETTINGS: OverlayName = OverlayName::new("settings");
const PANEL: OverlayName = OverlayName::new("panel");
const BANNER: OverlayName = OverlayName::new("banner");
const DRAWER: OverlayName = OverlayName::new("drawer");
const SHEET: OverlayName = OverlayName::new("sheet");

fn reset() {
    with_named(HashMap::clear);
}

#[test]
fn a_name_resolves_to_one_shared_signal() {
    reset();
    let first = state(CONFIRM);
    let second = state(CONFIRM);
    first.set(true);
    assert!(second.get(), "both handles are the same signal");
    assert!(!state(OTHER).get(), "a different name is independent");
}

/// The ordering that makes named overlays worth having: something opens the name before the widget for it exists (a restored deep link, a shortcut fired during startup), and the widget still comes up open.
#[test]
fn opening_a_name_before_its_overlay_is_built_still_opens_it() {
    reset();
    open(SETTINGS);
    assert!(
        state(SETTINGS).get(),
        "a name can be opened before anything is built for it"
    );
}

#[test]
fn open_and_close_drive_the_same_state() {
    reset();
    assert!(!state(PANEL).peek(), "the overlay starts closed");
    open(PANEL);
    assert!(state(PANEL).peek(), "open drives the state");
    close(PANEL);
    assert!(!state(PANEL).peek(), "and close drives the same one back");
}

/// The state is a real signal, so a trigger that styles itself as active while its panel is up re-runs on open and close without any wiring of its own.
#[test]
fn the_state_is_reactive() {
    reset();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let s = seen.clone();
    reactive_core::effect(move || s.borrow_mut().push(state(BANNER).get()));
    open(BANNER);
    close(BANNER);
    assert_eq!(*seen.borrow(), vec![false, true, false]);
}

/// Two canvases showing the same tree, as a matrix of one preview does: a name opened in one must not open the overlay of that name in the other, nor in the tree around them.
#[test]
fn a_name_opened_in_one_surface_leaves_another_closed() {
    reset();
    let a = Surface::new();
    let b = Surface::new();
    a.with_entered(|| {
        open(CONFIRM);
    });
    b.with_entered(|| {
        assert!(!state(CONFIRM).peek(), "b's confirm is b's own");
        open(CONFIRM);
        close(CONFIRM);
    });
    a.with_entered(|| {
        assert!(state(CONFIRM).peek(), "closing b's leaves a's open");
    });
    assert!(
        !state(CONFIRM).peek(),
        "the ambient world has a name of its own"
    );
}

/// What a canvas builds reads its own surface's name, and an opener run with that surface entered — a press inside it — reaches it.
#[test]
fn a_canvas_built_against_a_name_follows_opens_from_inside_it_alone() {
    reset();
    let build = || {
        let held = state(DRAWER);
        let read = Rc::new(RefCell::new(None));
        let reading = Rc::clone(&read);
        let canvas = SurfaceCanvas::new(Size::new(100.0, 100.0), move || {
            reading.replace(Some(state(DRAWER)));
            Ok(box_item(Container::new(LayoutStyle::new(), Vec::new())?))
        })
        .unwrap();
        let inner = read.take().expect("the build read its name");
        (canvas, held, inner)
    };
    let (first, outer, first_drawer) = build();
    let (second, _, second_drawer) = build();
    first.with_entered(|| {
        open(DRAWER);
    });
    assert!(first_drawer.peek());
    assert!(!second_drawer.peek(), "the sibling canvas stays closed");
    assert!(!outer.peek(), "the tree around them stays closed");
    drop(second);
}

#[test]
fn a_surfaces_names_go_with_it() {
    reset();
    let surface = Surface::new();
    let held = surface.with_entered(|| state(SHEET));
    assert!(held.is_alive());
    drop(surface);
    assert!(
        !held.is_alive(),
        "the name's signal is freed with its surface"
    );
}

#[test]
fn a_named_control_drives_the_names_state() {
    reset();
    let control = OverlayControl::from(PANEL);
    open(PANEL);
    assert!(control.state().peek(), "opening the name opens the control");
    control.state().set(false);
    assert!(
        !state(PANEL).peek(),
        "and closing the control closes the name"
    );
}

#[test]
fn a_signal_control_drives_its_own_signal() {
    reset();
    let held = signal(false);
    OverlayControl::from(held).state().set(true);
    assert!(held.peek(), "the control writes the caller's signal");
}
