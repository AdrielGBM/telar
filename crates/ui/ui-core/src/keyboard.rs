//! The keyboard as state rather than as events.
//!
//! Widgets are written against events — a press arrives, a handler runs — and that is the right shape for a button. It is the wrong shape for two other questions that come up constantly:
//!
//! - *"was `Shift` down when that click happened?"* A pointer event carries no modifiers, and a bare `Shift` press produces no key event to have tracked, so the answer exists nowhere in the event stream.
//! - *"is `ArrowUp` held **right now**?"* Asked once per frame by anything that acts for as long as a key is down rather than at the moment it went down — a camera orbiting, a value stepping, a sprite walking.
//!
//! Both are answered by keeping the state as the events go past, which is what this does. The modifier half is fed by [`Event::ModifiersChanged`], which the platform layer re-sends on focus changes — so it stays right across the alt-tab-mid-chord case that reconstruction gets wrong.

use platform_core::{Event, Key, ModifiersState};
use rustc_hash::FxHashSet;
use std::cell::RefCell;

#[derive(Default)]
struct Keyboard {
    modifiers: ModifiersState,
    held: FxHashSet<Key>,
    pressed: FxHashSet<Key>,
    last_press: Option<(Key, Key)>,
}

thread_local! {
    static KEYBOARD: RefCell<Keyboard> = RefCell::new(Keyboard::default());
}

/// Records what `event` says about the keyboard. The runner calls this for every event before dispatch.
pub fn observe(event: &Event) {
    if matches!(event, Event::KeyPressed { .. }) {
        crate::link::reader_moved();
    }
    KEYBOARD.with(|k| {
        let mut k = k.borrow_mut();
        match event {
            Event::ModifiersChanged { modifiers } => k.modifiers = *modifiers,
            Event::KeyPressed {
                key,
                modifiers,
                unmodified,
            } => {
                k.modifiers = *modifiers;
                k.last_press = unmodified
                    .clone()
                    .map(|unmodified| (key.clone(), unmodified));
                // `insert` reports whether the key was absent, which separates a first press from an OS repeat.
                if k.held.insert(key.clone()) {
                    k.pressed.insert(key.clone());
                }
            }
            Event::KeyReleased { key, modifiers } => {
                k.modifiers = *modifiers;
                k.held.remove(key);
            }
            // A window that loses focus never sends the releases for what was held, and letting them rot would leave whatever they drive running.
            Event::FocusChanged { is_focused: false } => {
                k.held.clear();
                k.pressed.clear();
                k.modifiers = ModifiersState::default();
            }
            _ => {}
        }
    });
}

/// Forgets the presses that belong to the frame just finished. The runner calls this once per frame, after dispatch, so [`key_pressed`] answers for exactly one frame.
pub fn end_frame() {
    KEYBOARD.with(|k| k.borrow_mut().pressed.clear());
}

/// The modifier keys held right now.
///
/// Authoritative rather than reconstructed: it comes from the platform's own reading, including the one it re-sends when the window regains focus. Read it inside a pointer handler to tell a plain click from a `Shift`-click.
pub fn modifiers() -> ModifiersState {
    KEYBOARD.with(|k| k.borrow().modifiers)
}

/// Whether `key` is down right now, however long it has been down.
///
/// A release lets go of the key its press reported, which the backend pairs by physical key ([`platform_core::KeyPairing`]), so a `\` typed through `AltGr` comes up with its key even when `AltGr` came up first. Losing the window's focus lets go of everything.
pub fn key_held(key: &Key) -> bool {
    KEYBOARD.with(|k| k.borrow().held.contains(key))
}

/// The key a shortcut should match for the press that reported `key`: what the layout makes of that physical key with no modifier applied.
///
/// `key` is what the modifiers made of it, so Option+T on macOS is `†` and a chord matching on it never fires. This answers `t` while that press is the latest one, and `key` itself for anything else: a backend that cannot tell, or a key that is not the latest press.
pub fn shortcut_key(key: &Key) -> Key {
    KEYBOARD.with(|k| match &k.borrow().last_press {
        Some((pressed, unmodified)) if pressed == key => unmodified.clone(),
        _ => key.clone(),
    })
}

/// Whether `key` went down during this frame. False for a key the OS is repeating, which is what makes it the one to drive a once-per-press action while [`key_held`] drives a continuous one.
pub fn key_pressed(key: &Key) -> bool {
    KEYBOARD.with(|k| k.borrow().pressed.contains(key))
}

/// Drops all keyboard state; parallels the other per-tree resets on teardown and hot reload.
pub fn reset() {
    KEYBOARD.with(|k| *k.borrow_mut() = Keyboard::default());
}

#[cfg(test)]
#[path = "keyboard_test.rs"]
mod tests;
