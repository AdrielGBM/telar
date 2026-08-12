//! The keyboard as state rather than as events.
//!
//! Widgets are written against events — a press arrives, a handler runs — and that is the right shape for
//! a button. It is the wrong shape for two other questions that come up constantly:
//!
//! - *"was `Shift` down when that click happened?"* A pointer event carries no modifiers, and a bare
//!   `Shift` press produces no key event to have tracked, so the answer exists nowhere in the event stream.
//! - *"is `ArrowUp` held **right now**?"* Asked once per frame by anything that acts for as long as a key is
//!   down rather than at the moment it went down — a camera orbiting, a value stepping, a sprite walking.
//!
//! Both are answered by keeping the state as the events go past, which is what this does. The modifier half
//! is fed by [`Event::ModifiersChanged`], which the platform layer re-sends on focus changes — so it stays
//! right across the alt-tab-mid-chord case that reconstruction gets wrong.

use platform_core::{Event, Key, ModifiersState};
use rustc_hash::FxHashSet;
use std::cell::RefCell;

#[derive(Default)]
struct Keyboard {
    modifiers: ModifiersState,
    held: FxHashSet<Key>,
    pressed: FxHashSet<Key>,
}

thread_local! {
    static KEYBOARD: RefCell<Keyboard> = RefCell::new(Keyboard::default());
}

/// Records what `event` says about the keyboard. The runner calls this for every event before dispatch.
pub fn observe(event: &Event) {
    KEYBOARD.with(|k| {
        let mut k = k.borrow_mut();
        match event {
            Event::ModifiersChanged { modifiers } => k.modifiers = *modifiers,
            Event::KeyPressed { key, modifiers } => {
                k.modifiers = *modifiers;
                // `insert` reports whether the key was absent, which is what separates a first press from the OS repeating one that was already down.
                if k.held.insert(key.clone()) {
                    k.pressed.insert(key.clone());
                }
            }
            Event::KeyReleased { key, modifiers } => {
                k.modifiers = *modifiers;
                k.held.remove(key);
            }
            // A window that loses focus never sends the releases for what was held, and the keys are not held any more by the time it comes back. Letting them rot would leave whatever they drive running until the user pressed and released the same key again.
            Event::FocusChanged { is_focused: false } => {
                k.held.clear();
                k.pressed.clear();
                k.modifiers = ModifiersState::default();
            }
            _ => {}
        }
    });
}

/// Forgets the presses that belong to the frame just finished. The runner calls this once per frame,
/// after dispatch, so [`key_pressed`] answers for exactly one frame.
pub fn end_frame() {
    KEYBOARD.with(|k| k.borrow_mut().pressed.clear());
}

/// The modifier keys held right now.
///
/// Authoritative rather than reconstructed: it comes from the platform's own reading, including the one it
/// re-sends when the window regains focus. Read it inside a pointer handler to tell a plain click from a
/// `Shift`-click.
pub fn modifiers() -> ModifiersState {
    KEYBOARD.with(|k| k.borrow().modifiers)
}

/// Whether `key` is down right now, however long it has been down.
pub fn key_held(key: &Key) -> bool {
    KEYBOARD.with(|k| k.borrow().held.contains(key))
}

/// Whether `key` went down during this frame. False for a key the OS is repeating, which is what makes it
/// the one to drive a once-per-press action while [`key_held`] drives a continuous one.
pub fn key_pressed(key: &Key) -> bool {
    KEYBOARD.with(|k| k.borrow().pressed.contains(key))
}

/// Drops all keyboard state; parallels the other per-tree resets on teardown and hot reload.
pub fn reset() {
    KEYBOARD.with(|k| *k.borrow_mut() = Keyboard::default());
}

#[cfg(test)]
mod tests {
    use super::*;
    use platform_core::NamedKey;

    fn up() -> Key {
        Key::Named(NamedKey::ArrowUp)
    }

    fn shift() -> ModifiersState {
        ModifiersState {
            is_shift: true,
            ..ModifiersState::default()
        }
    }

    fn fresh() {
        reset();
    }

    #[test]
    fn a_key_stays_held_until_it_is_released() {
        fresh();
        assert!(!key_held(&up()));
        observe(&Event::KeyPressed {
            key: up(),
            modifiers: ModifiersState::default(),
        });
        assert!(key_held(&up()));
        end_frame();
        assert!(key_held(&up()), "holding outlives the frame it began in");
        observe(&Event::KeyReleased {
            key: up(),
            modifiers: ModifiersState::default(),
        });
        assert!(!key_held(&up()));
    }

    #[test]
    fn a_press_answers_for_one_frame_only() {
        fresh();
        observe(&Event::KeyPressed {
            key: up(),
            modifiers: ModifiersState::default(),
        });
        assert!(key_pressed(&up()));
        end_frame();
        assert!(!key_pressed(&up()));
    }

    /// The OS repeats a held key as fresh presses. Counting them would fire a once-per-press action forty
    /// times a second for a user who simply never let go.
    #[test]
    fn a_repeated_key_is_not_a_new_press() {
        fresh();
        observe(&Event::KeyPressed {
            key: up(),
            modifiers: ModifiersState::default(),
        });
        end_frame();
        observe(&Event::KeyPressed {
            key: up(),
            modifiers: ModifiersState::default(),
        });
        assert!(key_held(&up()));
        assert!(!key_pressed(&up()), "the key never came back up");
    }

    /// The case the whole module exists for: `Shift` alone maps to no `Key`, so without its own event the
    /// state would still read whatever the last typed character carried.
    #[test]
    fn a_bare_modifier_is_visible_without_any_key_event() {
        fresh();
        assert!(!modifiers().is_shift);
        observe(&Event::ModifiersChanged { modifiers: shift() });
        assert!(modifiers().is_shift);
    }

    /// Losing focus mid-chord is exactly where a reconstructed state goes wrong: the releases never come.
    #[test]
    fn losing_focus_forgets_what_was_held() {
        fresh();
        observe(&Event::KeyPressed {
            key: up(),
            modifiers: shift(),
        });
        assert!(key_held(&up()) && modifiers().is_shift);
        observe(&Event::FocusChanged { is_focused: false });
        assert!(!key_held(&up()));
        assert!(!modifiers().is_shift);
    }
}
