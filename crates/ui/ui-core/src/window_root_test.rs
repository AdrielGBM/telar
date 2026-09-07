use platform_core::{PointerButton, PointerSource};
use reactive_core::signal;
use renderer_core::{Color, TextStyle};

use super::*;
use crate::Input;
use crate::context::reset_layout_runtime;
use crate::focus;

fn press(at: (f64, f64)) -> Event {
    Event::PointerPressed {
        x: at.0,
        y: at.1,
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    }
}

/// A field 100x20 in the top-left of a 400x300 window, holding the keyboard.
fn window_with_a_focused_field() -> WindowRoot {
    reset_layout_runtime();
    focus::clear();
    let field = Input::new(
        signal(String::new()),
        LayoutStyle::new().width(100.0).height(20.0),
        || TextStyle::new(14.0, Color::BLACK),
    )
    .unwrap()
    .autofocus();
    let mut root = WindowRoot::new(crate::box_item(field));
    root.on_event(&Event::WindowResized {
        width: 400,
        height: 300,
    });
    assert!(focus::current().is_some(), "the field opens holding it");
    root
}

/// **A press that lands on nothing focusable takes the keyboard away.**
///
/// Focus was only ever *taken* — by a tap on a focusable, by Tab — so clicking away from a form left the field with the caret still in it, still eating the keys, and still answering «somebody is typing» to an application asking whether to run its shortcuts.
#[test]
fn a_press_on_nothing_lets_the_keyboard_go() {
    let mut root = window_with_a_focused_field();

    root.on_event(&press((300.0, 200.0)));

    assert!(focus::current().is_none(), "el clic fuera no soltó el foco");
}

/// And the other half, which is what keeps the rule from being a nuisance: a press that lands *on* the field leaves it exactly where it was.
#[test]
fn a_press_on_the_field_leaves_it_holding_the_keyboard() {
    let mut root = window_with_a_focused_field();
    let held = focus::current();

    root.on_event(&press((50.0, 10.0)));

    assert_eq!(focus::current(), held, "el clic dentro le quitó el foco");
}
