use winit::event::{Modifiers, MouseButton as WinitMouseButton};
use winit::keyboard::NamedKey as WinitNamedKey;

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
