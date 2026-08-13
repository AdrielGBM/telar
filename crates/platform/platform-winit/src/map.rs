use winit::event::{Modifiers, MouseButton as WinitMouseButton};
use winit::keyboard::{Key as WinitKey, KeyLocation, NamedKey as WinitNamedKey};

/// Translates a winit logical key into a [`platform_core::Key`], resolving the keypad from `location`.
///
/// Both backends go through here so a key can never reach one and not the other. `location` is what
/// separates the keypad from the digit row: winit reports `Numpad7` as the character `"7"` and only the
/// location says which key was struck.
pub fn map_key(key: &WinitKey, location: KeyLocation) -> Option<platform_core::Key> {
    if location == KeyLocation::Numpad
        && let Some(nk) = map_numpad(key)
    {
        return Some(platform_core::Key::Named(nk));
    }
    match key {
        WinitKey::Character(c) => c.as_str().chars().next().map(platform_core::Key::Char),
        WinitKey::Named(named) => map_named_key(*named).map(platform_core::Key::Named),
        _ => None,
    }
}

fn map_numpad(key: &WinitKey) -> Option<platform_core::NamedKey> {
    use platform_core::NamedKey as Nk;
    let nk = match key {
        WinitKey::Named(WinitNamedKey::Enter) => Nk::NumpadEnter,
        WinitKey::Character(c) => match c.as_str() {
            "0" => Nk::Numpad0,
            "1" => Nk::Numpad1,
            "2" => Nk::Numpad2,
            "3" => Nk::Numpad3,
            "4" => Nk::Numpad4,
            "5" => Nk::Numpad5,
            "6" => Nk::Numpad6,
            "7" => Nk::Numpad7,
            "8" => Nk::Numpad8,
            "9" => Nk::Numpad9,
            "+" => Nk::NumpadAdd,
            "-" => Nk::NumpadSubtract,
            "*" => Nk::NumpadMultiply,
            "/" => Nk::NumpadDivide,
            // The separator is a comma on a keyboard whose locale writes decimals that way.
            "." | "," => Nk::NumpadDecimal,
            _ => return None,
        },
        _ => return None,
    };
    Some(nk)
}

// Shared winit->platform_core translation used by every winit backend (desktop + android). Keeping the
// NamedKey table in one place avoids the two-backend hazard where a new key is added to one match only.
pub fn map_named_key(key: WinitNamedKey) -> Option<platform_core::NamedKey> {
    let nk = match key {
        WinitNamedKey::Enter => platform_core::NamedKey::Enter,
        WinitNamedKey::Backspace => platform_core::NamedKey::Backspace,
        WinitNamedKey::Escape => platform_core::NamedKey::Escape,
        WinitNamedKey::Tab => platform_core::NamedKey::Tab,
        WinitNamedKey::Delete => platform_core::NamedKey::Delete,
        WinitNamedKey::Home => platform_core::NamedKey::Home,
        WinitNamedKey::End => platform_core::NamedKey::End,
        WinitNamedKey::PageUp => platform_core::NamedKey::PageUp,
        WinitNamedKey::PageDown => platform_core::NamedKey::PageDown,
        WinitNamedKey::ArrowUp => platform_core::NamedKey::ArrowUp,
        WinitNamedKey::ArrowDown => platform_core::NamedKey::ArrowDown,
        WinitNamedKey::ArrowLeft => platform_core::NamedKey::ArrowLeft,
        WinitNamedKey::ArrowRight => platform_core::NamedKey::ArrowRight,
        WinitNamedKey::F1 => platform_core::NamedKey::F1,
        WinitNamedKey::F2 => platform_core::NamedKey::F2,
        WinitNamedKey::F3 => platform_core::NamedKey::F3,
        WinitNamedKey::F4 => platform_core::NamedKey::F4,
        WinitNamedKey::F5 => platform_core::NamedKey::F5,
        WinitNamedKey::F6 => platform_core::NamedKey::F6,
        WinitNamedKey::F7 => platform_core::NamedKey::F7,
        WinitNamedKey::F8 => platform_core::NamedKey::F8,
        WinitNamedKey::F9 => platform_core::NamedKey::F9,
        WinitNamedKey::F10 => platform_core::NamedKey::F10,
        WinitNamedKey::F11 => platform_core::NamedKey::F11,
        WinitNamedKey::F12 => platform_core::NamedKey::F12,
        WinitNamedKey::F13 => platform_core::NamedKey::F13,
        WinitNamedKey::F14 => platform_core::NamedKey::F14,
        WinitNamedKey::F15 => platform_core::NamedKey::F15,
        WinitNamedKey::F16 => platform_core::NamedKey::F16,
        WinitNamedKey::F17 => platform_core::NamedKey::F17,
        WinitNamedKey::F18 => platform_core::NamedKey::F18,
        WinitNamedKey::F19 => platform_core::NamedKey::F19,
        WinitNamedKey::F20 => platform_core::NamedKey::F20,
        WinitNamedKey::F21 => platform_core::NamedKey::F21,
        WinitNamedKey::F22 => platform_core::NamedKey::F22,
        WinitNamedKey::F23 => platform_core::NamedKey::F23,
        WinitNamedKey::F24 => platform_core::NamedKey::F24,
        WinitNamedKey::Space => platform_core::NamedKey::Space,
        WinitNamedKey::Insert => platform_core::NamedKey::Insert,
        WinitNamedKey::CapsLock => platform_core::NamedKey::CapsLock,
        _ => return None,
    };
    Some(nk)
}

pub fn map_mouse_button(button: WinitMouseButton) -> Option<platform_core::PointerButton> {
    match button {
        WinitMouseButton::Left => Some(platform_core::PointerButton::Primary),
        WinitMouseButton::Right => Some(platform_core::PointerButton::Secondary),
        WinitMouseButton::Middle => Some(platform_core::PointerButton::Auxiliary),
        _ => None,
    }
}

pub fn map_modifiers(mods: &Modifiers) -> platform_core::ModifiersState {
    platform_core::ModifiersState {
        is_shift: mods.state().shift_key(),
        is_ctrl: mods.state().control_key(),
        is_alt: mods.state().alt_key(),
        is_meta: mods.state().super_key(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use platform_core::{Key, NamedKey};
    use winit::keyboard::SmolStr;

    fn character(s: &str) -> WinitKey {
        WinitKey::Character(SmolStr::new(s))
    }

    /// The keypad and the digit row send the same logical key; only the location tells them apart. An
    /// application that binds Numpad 7 to a view means that key and not the 7 above the letters.
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

    /// With Num Lock off the OS says the keypad's 1 is `End`, and that is what it reports: overriding it
    /// would take the arrows away from someone navigating with the keypad.
    #[test]
    fn a_keypad_key_without_num_lock_stays_what_the_os_calls_it() {
        assert_eq!(
            map_key(&WinitKey::Named(WinitNamedKey::End), KeyLocation::Numpad),
            Some(Key::Named(NamedKey::End))
        );
    }
}
