use super::*;

struct Blank;

impl crate::app::App for Blank {
    fn root(&self) -> Box<dyn Component> {
        ui_core::reset_layout_runtime();
        Box::new(
            ui_core::Rectangle::new(
                layout_core::LayoutStyle::new().width(10.0).height(10.0),
                || renderer_core::RectStyle::filled(renderer_core::Color::BLACK, 0.0),
            )
            .expect("a rectangle builds"),
        )
    }
}

/// A tree behind the hot-reload boundary has to feed the input registries **its own** widgets read.
///
/// A `cdylib` carries its own copy of every `thread_local` in `ui-core`, so the runner observing on the host side filled a registry nothing in the app ever looked at: `modifiers()` answered "none held" and `pointer_buttons()` answered "nothing pressed", confidently, for a whole `cargo telar dev` session. A ⇧-click was a plain click and a right-drag was a left-drag, and every gesture built on either did the wrong thing in the window while passing its own tests — which is exactly the shape of failure a state registry has, because it never says it does not know.
///
/// The boundary itself cannot be built in a unit test; the property that fixes it can: whoever dispatches an event observes it.
#[test]
fn the_hot_tree_feeds_the_registries_its_own_widgets_read() {
    ui_core::reset_keyboard();
    ui_core::reset_pointer();
    let tree = HotTree::mount(&Blank);

    assert!(
        !ui_core::modifiers().is_shift,
        "shift is not down before the event"
    );
    unsafe {
        HotTree::on_event(
            tree,
            &Event::ModifiersChanged {
                modifiers: platform_core::ModifiersState {
                    is_shift: true,
                    ..Default::default()
                },
            },
        );
    }
    assert!(
        ui_core::modifiers().is_shift,
        "the side that dispatched has to be the side that knows"
    );

    unsafe {
        HotTree::on_event(
            tree,
            &Event::PointerPressed {
                x: 5.0,
                y: 5.0,
                button: platform_core::PointerButton::Secondary,
                source: platform_core::PointerSource::Mouse,
            },
        );
    }
    assert!(
        ui_core::pointer_buttons().secondary,
        "and the same for which button is down"
    );
    unsafe { HotTree::release(tree) };
    ui_core::reset_keyboard();
    ui_core::reset_pointer();
}
