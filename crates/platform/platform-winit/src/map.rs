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
/// Lines one detent of a mouse wheel is worth.
///
/// Every winit backend reports a notch as one line, and one line is a fifth of what a browser moves for the
/// same flick of the finger — which is what turning a wheel here felt like against any other window on the
/// screen. The number is the browsers' hundred pixels over the runtime's twenty-pixel line.
const LINES_PER_NOTCH: f32 = 5.0;

pub enum SurfaceIntent {
    // Deliver this platform event to the handler.
    Event(Event),
    // Deliver this platform event, then request a redraw (winit `Resized`).
    Resized(Event),
    // Render now (winit `RedrawRequested`).
    Redraw,
    // Deliver `WindowCloseRequested`, then close this surface.
    Close(Event),
    // A finger that moved: the scroll it amounts to, then the move itself.
    //
    // Both, because a drag is both. The list under the finger scrolls while a slider under that same finger
    // tracks the movement, and a backend delivering only one of the two broke whichever widget needed the other.
    Dragged(Event, Event),
    // State-only (e.g. `ModifiersChanged`) or an unmapped event — nothing to deliver.
    Ignore,
}

/// Where a finger was last seen, so the next report can be told as the distance it covered.
///
/// A touch screen reports positions; a scroll is made of deltas. Nothing in winit turns one into the other, so
/// a backend wanting a finger to scroll has to remember the last point itself — and while only one of them did,
/// dragging a list moved nothing anywhere else.
#[derive(Default)]
pub struct TouchDrag {
    last: Option<(f64, f64, u64)>,
}

impl TouchDrag {
    // How far this finger has come since it was last seen. `None` for one arriving mid-gesture, and for a
    // second finger landing while the first is still down: the distance between two fingers is not a scroll.
    fn advance(&mut self, x: f64, y: f64, id: u64) -> Option<(f32, f32)> {
        let moved = self
            .last
            .and_then(|(lx, ly, lid)| (lid == id).then_some(((x - lx) as f32, (y - ly) as f32)));
        self.last = Some((x, y, id));
        moved
    }

    fn end(&mut self) {
        self.last = None;
    }
}

// Pure winit `WindowEvent` → [`SurfaceIntent`] translation, updating this surface's cursor/scale/modifiers.
// No handler and no window side effects, so it can run on the winit thread while the handler lives elsewhere.
pub fn map_window_event(
    event: WindowEvent,
    cursor_position: &mut (f64, f64),
    scale_factor: &mut f64,
    modifiers: &mut platform_core::ModifiersState,
    touch: &mut TouchDrag,
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
            match phase {
                TouchPhase::Started => {
                    touch.advance(x, y, id);
                    SurfaceIntent::Event(Event::PointerPressed {
                        x,
                        y,
                        button: PointerButton::Primary,
                        source,
                    })
                }
                TouchPhase::Moved => {
                    let moved = Event::PointerMoved { x, y, source };
                    match touch.advance(x, y, id) {
                        Some((dx, dy)) => SurfaceIntent::Dragged(
                            Event::Scrolled {
                                delta: ScrollDelta::Pixels { x: dx, y: dy },
                                x,
                                y,
                            },
                            moved,
                        ),
                        None => SurfaceIntent::Event(moved),
                    }
                }
                TouchPhase::Ended | TouchPhase::Cancelled => {
                    touch.end();
                    SurfaceIntent::Event(Event::PointerReleased {
                        x,
                        y,
                        button: PointerButton::Primary,
                        source,
                    })
                }
            }
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
        WindowEvent::MouseWheel { delta, phase, .. } => {
            let (x, y) = *cursor_position;
            // Fingers lifting: the gesture is over and what it was doing is now the scroll's to carry on.
            if phase == TouchPhase::Ended {
                return SurfaceIntent::Event(Event::ScrollEnded { x, y });
            }
            let scroll_delta = match delta {
                MouseScrollDelta::LineDelta(x, y) => ScrollDelta::Lines {
                    x: x * LINES_PER_NOTCH,
                    y: y * LINES_PER_NOTCH,
                },
                MouseScrollDelta::PixelDelta(pos) => ScrollDelta::Pixels {
                    x: (pos.x / *scale_factor) as f32,
                    y: (pos.y / *scale_factor) as f32,
                },
            };
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

#[cfg(test)]
mod touch_tests {
    use super::*;

    fn drag(touch: &mut TouchDrag, x: f64, y: f64) -> Option<(f32, f32)> {
        touch.advance(x, y, 1)
    }

    #[test]
    fn a_finger_that_moves_covers_the_distance_between_its_reports() {
        let mut touch = TouchDrag::default();
        assert_eq!(
            drag(&mut touch, 100.0, 200.0),
            None,
            "nothing to measure yet"
        );
        assert_eq!(drag(&mut touch, 100.0, 180.0), Some((0.0, -20.0)));
        assert_eq!(drag(&mut touch, 90.0, 170.0), Some((-10.0, -10.0)));
    }

    #[test]
    fn a_finger_lifting_leaves_nothing_behind_for_the_next_one() {
        let mut touch = TouchDrag::default();
        drag(&mut touch, 0.0, 0.0);
        drag(&mut touch, 0.0, 50.0);
        touch.end();
        // Without the reset the next gesture opens with the jump from wherever the last one ended, which on a
        // long page is the whole list moving at once.
        assert_eq!(drag(&mut touch, 0.0, 400.0), None);
    }

    #[test]
    fn a_second_finger_landing_is_not_a_distance_from_the_first() {
        let mut touch = TouchDrag::default();
        touch.advance(0.0, 0.0, 1);
        assert_eq!(touch.advance(300.0, 0.0, 2), None);
        assert_eq!(touch.advance(300.0, 40.0, 2), Some((0.0, 40.0)));
    }
}
