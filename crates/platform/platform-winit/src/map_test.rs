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
