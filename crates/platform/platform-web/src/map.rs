//! Browser input, in Telar's event vocabulary.

use platform_core::{
    Event, Key, ModifiersState, NamedKey, PointerButton, PointerSource, ScrollDelta,
};

pub fn modifiers_of(shift: bool, ctrl: bool, alt: bool, meta: bool) -> ModifiersState {
    ModifiersState {
        is_shift: shift,
        is_ctrl: ctrl,
        is_alt: alt,
        is_meta: meta,
    }
}

pub fn mouse_modifiers(event: &web_sys::MouseEvent) -> ModifiersState {
    modifiers_of(
        event.shift_key(),
        event.ctrl_key(),
        event.alt_key(),
        event.meta_key(),
    )
}

pub fn key_modifiers(event: &web_sys::KeyboardEvent) -> ModifiersState {
    modifiers_of(
        event.shift_key(),
        event.ctrl_key(),
        event.alt_key(),
        event.meta_key(),
    )
}

pub fn button_of(button: i16) -> Option<PointerButton> {
    match button {
        0 => Some(PointerButton::Primary),
        1 => Some(PointerButton::Auxiliary),
        2 => Some(PointerButton::Secondary),
        _ => None,
    }
}

/// A pointer that is not a mouse is a touch, whatever the browser calls it: a pen and a finger both want the
/// behaviour a touch gets — no hover, and a gesture that begins where it lands.
pub fn source_of(event: &web_sys::PointerEvent) -> PointerSource {
    match event.pointer_type().as_str() {
        "mouse" => PointerSource::Mouse,
        _ => PointerSource::Touch {
            id: event.pointer_id() as u64,
        },
    }
}

/// `WheelEvent.deltaMode`, whose values the web-sys binding does not name.
const DELTA_PIXEL: u32 = 0;
const DELTA_LINE: u32 = 1;
const DELTA_PAGE: u32 = 2;

/// How far a wheel event scrolls. The browser reports three units and the choice is the *device's*, not the
/// page's — a trackpad reports pixels and a notched wheel reports lines — so both are passed through as what
/// they are rather than flattened into one.
pub fn scroll_delta(event: &web_sys::WheelEvent) -> ScrollDelta {
    // The browser's axes point the way the content moves; Telar's point the way the *gesture* does.
    let (x, y) = (-event.delta_x() as f32, -event.delta_y() as f32);
    match event.delta_mode() {
        DELTA_LINE => ScrollDelta::Lines { x, y },
        DELTA_PAGE => ScrollDelta::Lines {
            x: x * PAGE_LINES,
            y: y * PAGE_LINES,
        },
        DELTA_PIXEL | _ => ScrollDelta::Pixels { x, y },
    }
}

/// How many lines a page-mode wheel notch is worth. The browsers that still report pages use it for
/// PageUp/PageDown-sized jumps, and a screenful of a list is about this many rows.
const PAGE_LINES: f32 = 20.0;

/// A `KeyboardEvent.key` value, as a Telar key.
///
/// `key` rather than `code`, because it is the character the user meant: it already accounts for the layout
/// and the modifiers, so a French keyboard's `;` arrives as `;` and not as `Comma`.
pub fn key_of(key: &str) -> Option<Key> {
    let named = |k| Some(Key::Named(k));
    match key {
        "Enter" => named(NamedKey::Enter),
        "Backspace" => named(NamedKey::Backspace),
        "Escape" => named(NamedKey::Escape),
        "Tab" => named(NamedKey::Tab),
        "Delete" => named(NamedKey::Delete),
        "Home" => named(NamedKey::Home),
        "End" => named(NamedKey::End),
        "PageUp" => named(NamedKey::PageUp),
        "PageDown" => named(NamedKey::PageDown),
        "ArrowUp" => named(NamedKey::ArrowUp),
        "ArrowDown" => named(NamedKey::ArrowDown),
        "ArrowLeft" => named(NamedKey::ArrowLeft),
        "ArrowRight" => named(NamedKey::ArrowRight),
        "Insert" => named(NamedKey::Insert),
        "CapsLock" => named(NamedKey::CapsLock),
        " " => named(NamedKey::Space),
        _ => {
            if let Some(n) = key.strip_prefix('F').and_then(|n| n.parse::<u8>().ok()) {
                return function_key(n);
            }
            // Everything else that is exactly one character is that character. A longer `key` is a name this
            // does not map — "AltGraph", "MediaPlayPause" — and inventing a `Char` for it would type it.
            let mut chars = key.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => Some(Key::Char(c)),
                _ => None,
            }
        }
    }
}

fn function_key(n: u8) -> Option<Key> {
    const KEYS: [NamedKey; 24] = [
        NamedKey::F1,
        NamedKey::F2,
        NamedKey::F3,
        NamedKey::F4,
        NamedKey::F5,
        NamedKey::F6,
        NamedKey::F7,
        NamedKey::F8,
        NamedKey::F9,
        NamedKey::F10,
        NamedKey::F11,
        NamedKey::F12,
        NamedKey::F13,
        NamedKey::F14,
        NamedKey::F15,
        NamedKey::F16,
        NamedKey::F17,
        NamedKey::F18,
        NamedKey::F19,
        NamedKey::F20,
        NamedKey::F21,
        NamedKey::F22,
        NamedKey::F23,
        NamedKey::F24,
    ];
    KEYS.get(n.checked_sub(1)? as usize)
        .cloned()
        .map(Key::Named)
}

/// Whether the browser's default action for a key would fight the app for it.
///
/// Tab moves focus out of the surface, the arrows and space scroll the page, and Backspace navigates back in
/// some browsers — all while the app is using them. Only suppressed while the surface has focus, which is
/// what keeps the rest of the page usable.
pub fn key_steals_default(key: &Key) -> bool {
    matches!(
        key,
        Key::Named(
            NamedKey::Tab
                | NamedKey::Space
                | NamedKey::ArrowUp
                | NamedKey::ArrowDown
                | NamedKey::ArrowLeft
                | NamedKey::ArrowRight
                | NamedKey::PageUp
                | NamedKey::PageDown
                | NamedKey::Home
                | NamedKey::End
                | NamedKey::Backspace
        )
    )
}

/// The events one `pointerdown` is worth. The browser reports the press at a position without a preceding
/// move for a touch, so a widget that has never seen the pointer would take a press it never highlighted for.
pub fn pointer_pressed(x: f64, y: f64, button: PointerButton, source: PointerSource) -> [Event; 2] {
    [
        Event::PointerMoved {
            x,
            y,
            source: source.clone(),
        },
        Event::PointerPressed {
            x,
            y,
            button,
            source,
        },
    ]
}
