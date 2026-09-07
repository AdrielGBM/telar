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
