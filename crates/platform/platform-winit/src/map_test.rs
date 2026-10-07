use super::*;
use platform_core::{Key, NamedKey};
use winit::keyboard::SmolStr;

fn character(s: &str) -> WinitKey {
    WinitKey::Character(SmolStr::new(s))
}

/// The keypad and the digit row send the same logical key; only the location tells them apart. An application that binds Numpad 7 to a view means that key and not the 7 above the letters.
#[test]
fn the_keypad_is_its_own_set_of_keys() {
    assert_eq!(
        map_key(&character("7"), KeyLocation::Numpad),
        Some(Key::Named(NamedKey::Numpad7))
    );
    assert_eq!(
        map_key(&character("7"), KeyLocation::Standard),
        Some(Key::Char('7'))
    );
    assert_eq!(
        map_key(&WinitKey::Named(WinitNamedKey::Enter), KeyLocation::Numpad),
        Some(Key::Named(NamedKey::NumpadEnter))
    );
}

/// With Num Lock off the OS says the keypad's 1 is `End`, and that is what it reports: overriding it would take the arrows away from someone navigating with the keypad.
#[test]
fn a_keypad_key_without_num_lock_stays_what_the_os_calls_it() {
    assert_eq!(
        map_key(&WinitKey::Named(WinitNamedKey::End), KeyLocation::Numpad),
        Some(Key::Named(NamedKey::End))
    );
}

fn map_alone(event: WindowEvent) -> SurfaceIntent {
    map_window_event(
        event,
        &mut (0.0, 0.0),
        &mut 1.0,
        &mut platform_core::ModifiersState::default(),
        &mut TouchDrag::default(),
        &mut KeyPairing::default(),
    )
}

fn mouse(button: WinitMouseButton, state: ElementState) -> WindowEvent {
    WindowEvent::MouseInput {
        device_id: winit::event::DeviceId::dummy(),
        state,
        button,
    }
}

/// A mouse's back button is a request to go back, not a fourth pointer button a widget could arm.
#[test]
fn the_back_button_asks_to_go_back_once_per_press() {
    assert!(matches!(
        map_alone(mouse(WinitMouseButton::Back, ElementState::Pressed)),
        SurfaceIntent::Back
    ));
    assert!(matches!(
        map_alone(mouse(WinitMouseButton::Back, ElementState::Released)),
        SurfaceIntent::Ignore
    ));
    assert!(matches!(
        map_alone(mouse(WinitMouseButton::Left, ElementState::Pressed)),
        SurfaceIntent::Event(Event::PointerPressed { .. })
    ));
}

/// The menu key is how a keyboard opens a context menu, so it arrives named rather than dropped.
#[test]
fn the_menu_key_maps() {
    assert_eq!(
        map_key(
            &WinitKey::Named(WinitNamedKey::ContextMenu),
            KeyLocation::Standard
        ),
        Some(Key::Named(NamedKey::ContextMenu))
    );
}

fn released_as(intent: SurfaceIntent) -> Option<Key> {
    match intent {
        SurfaceIntent::Event(Event::KeyReleased { key, .. }) => Some(key),
        _ => None,
    }
}

/// `AltGr` let go before the key it modified: the layout reads the release as the key's plain level, and the release still names the `\` that went down.
#[test]
fn a_release_names_the_key_its_physical_press_produced() {
    let mut keys = KeyPairing::default();
    let backslash = PhysicalKey::Code(winit::keyboard::KeyCode::Backquote);
    let modifiers = platform_core::ModifiersState::default();
    assert!(matches!(
        paired_key_event(
            &mut keys,
            backslash,
            ElementState::Pressed,
            Some(Key::Char('\\')),
            modifiers
        ),
        SurfaceIntent::Event(Event::KeyPressed {
            key: Key::Char('\\'),
            ..
        })
    ));
    let released = paired_key_event(
        &mut keys,
        backslash,
        ElementState::Released,
        Some(Key::Char('º')),
        modifiers,
    );
    assert_eq!(released_as(released), Some(Key::Char('\\')));
}

/// A window that loses the keyboard never hears the releases, so what it remembers as down is forgotten with it.
#[test]
fn losing_focus_forgets_what_was_down() {
    let mut keys = KeyPairing::default();
    let backslash = PhysicalKey::Code(winit::keyboard::KeyCode::Backquote);
    let modifiers = platform_core::ModifiersState::default();
    paired_key_event(
        &mut keys,
        backslash,
        ElementState::Pressed,
        Some(Key::Char('\\')),
        modifiers,
    );
    map_window_event(
        WindowEvent::Focused(false),
        &mut (0.0, 0.0),
        &mut 1.0,
        &mut platform_core::ModifiersState::default(),
        &mut TouchDrag::default(),
        &mut keys,
    );
    let released = paired_key_event(
        &mut keys,
        backslash,
        ElementState::Released,
        Some(Key::Char('º')),
        modifiers,
    );
    assert_eq!(released_as(released), Some(Key::Char('º')));
}
