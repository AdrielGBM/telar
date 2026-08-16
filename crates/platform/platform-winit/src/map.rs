use platform_core::{Event, PointerButton, PointerSource, ScrollDelta};
use winit::event::{ElementState, MouseScrollDelta, Touch, TouchPhase, WindowEvent};
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

// What a mapped winit `WindowEvent` means at the platform level, decoupled from *how* it's applied. The
// single-window runner applies it to a handler directly; the multi-window runner forwards it to that
// surface's worker thread. Keeping the mapping here (and the application at the call site) lets both share
// the exact same winit→platform translation.
pub enum SurfaceIntent {
    // Deliver this platform event to the handler.
    Event(Event),
    // Deliver this platform event, then request a redraw (winit `Resized`).
    Resized(Event),
    // Render now (winit `RedrawRequested`).
    Redraw,
    // Deliver `WindowCloseRequested`, then close this surface.
    Close(Event),
    // State-only (e.g. `ModifiersChanged`) or an unmapped event — nothing to deliver.
    Ignore,
}

// Pure winit `WindowEvent` → [`SurfaceIntent`] translation, updating this surface's cursor/scale/modifiers.
// No handler and no window side effects, so it can run on the winit thread while the handler lives elsewhere.
pub fn map_window_event(
    event: WindowEvent,
    cursor_position: &mut (f64, f64),
    scale_factor: &mut f64,
    modifiers: &mut platform_core::ModifiersState,
) -> SurfaceIntent {
    match event {
        WindowEvent::CloseRequested => SurfaceIntent::Close(Event::WindowCloseRequested),
        WindowEvent::Resized(size) => SurfaceIntent::Resized(Event::WindowResized {
            width: (size.width as f64 / *scale_factor).round() as u32,
            height: (size.height as f64 / *scale_factor).round() as u32,
        }),
        WindowEvent::RedrawRequested => SurfaceIntent::Redraw,
        WindowEvent::CursorMoved { position, .. } => {
            let lx = position.x / *scale_factor;
            let ly = position.y / *scale_factor;
            *cursor_position = (lx, ly);
            SurfaceIntent::Event(Event::PointerMoved {
                x: lx,
                y: ly,
                source: PointerSource::Mouse,
            })
        }
        WindowEvent::MouseInput { state, button, .. } => {
            let Some(btn) = crate::map_mouse_button(button) else {
                return SurfaceIntent::Ignore;
            };
            let (x, y) = *cursor_position;
            SurfaceIntent::Event(match state {
                ElementState::Pressed => Event::PointerPressed {
                    x,
                    y,
                    button: btn,
                    source: PointerSource::Mouse,
                },
                ElementState::Released => Event::PointerReleased {
                    x,
                    y,
                    button: btn,
                    source: PointerSource::Mouse,
                },
            })
        }
        WindowEvent::Touch(Touch {
            phase,
            location,
            id,
            ..
        }) => {
            let x = location.x / *scale_factor;
            let y = location.y / *scale_factor;
            let source = PointerSource::Touch { id };
            SurfaceIntent::Event(match phase {
                TouchPhase::Started => Event::PointerPressed {
                    x,
                    y,
                    button: PointerButton::Primary,
                    source,
                },
                TouchPhase::Moved => Event::PointerMoved { x, y, source },
                TouchPhase::Ended | TouchPhase::Cancelled => Event::PointerReleased {
                    x,
                    y,
                    button: PointerButton::Primary,
                    source,
                },
            })
        }
        WindowEvent::Focused(is_focused) => {
            SurfaceIntent::Event(Event::FocusChanged { is_focused })
        }
        WindowEvent::CursorEntered { .. } => SurfaceIntent::Event(Event::CursorEntered),
        WindowEvent::CursorLeft { .. } => SurfaceIntent::Event(Event::CursorLeft),
        WindowEvent::ScaleFactorChanged {
            scale_factor: new_scale,
            ..
        } => {
            *scale_factor = new_scale;
            SurfaceIntent::Event(Event::ScaleFactorChanged {
                scale_factor: new_scale,
            })
        }
        WindowEvent::MouseWheel { delta, .. } => {
            let scroll_delta = match delta {
                MouseScrollDelta::LineDelta(x, y) => ScrollDelta::Lines { x, y },
                MouseScrollDelta::PixelDelta(pos) => ScrollDelta::Pixels {
                    x: (pos.x / *scale_factor) as f32,
                    y: (pos.y / *scale_factor) as f32,
                },
            };
            let (x, y) = *cursor_position;
            SurfaceIntent::Event(Event::Scrolled {
                delta: scroll_delta,
                x,
                y,
            })
        }
        WindowEvent::ModifiersChanged(mods) => {
            *modifiers = crate::map_modifiers(&mods);
            SurfaceIntent::Event(Event::ModifiersChanged {
                modifiers: *modifiers,
            })
        }
        WindowEvent::KeyboardInput { event, .. } => {
            let Some(key) = crate::map_key(&event.logical_key, event.location) else {
                return SurfaceIntent::Ignore;
            };
            let mods = *modifiers;
            SurfaceIntent::Event(match event.state {
                ElementState::Pressed => Event::KeyPressed {
                    key,
                    modifiers: mods,
                },
                ElementState::Released => Event::KeyReleased {
                    key,
                    modifiers: mods,
                },
            })
        }
        WindowEvent::ThemeChanged(theme) => SurfaceIntent::Event(Event::ColorSchemeChanged {
            dark: theme == winit::window::Theme::Dark,
        }),
        _ => SurfaceIntent::Ignore,
    }
}
