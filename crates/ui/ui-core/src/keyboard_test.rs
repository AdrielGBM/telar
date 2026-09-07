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
    assert!(!key_held(&up()), "nothing is held before any event");
    observe(&Event::KeyPressed {
        key: up(),
        modifiers: ModifiersState::default(),
    });
    assert!(key_held(&up()), "a press holds the key down");
    end_frame();
    assert!(key_held(&up()), "holding outlives the frame it began in");
    observe(&Event::KeyReleased {
        key: up(),
        modifiers: ModifiersState::default(),
    });
    assert!(!key_held(&up()), "and the release lets it go");
}

#[test]
fn a_press_answers_for_one_frame_only() {
    fresh();
    observe(&Event::KeyPressed {
        key: up(),
        modifiers: ModifiersState::default(),
    });
    assert!(key_pressed(&up()), "the frame of the press answers yes");
    end_frame();
    assert!(!key_pressed(&up()), "and the next frame no longer does");
}

/// The OS repeats a held key as fresh presses. Counting them would fire a once-per-press action forty times a second for a user who simply never let go.
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
    assert!(key_held(&up()), "an auto-repeat leaves the key held");
    assert!(!key_pressed(&up()), "the key never came back up");
}

/// The case the whole module exists for: `Shift` alone maps to no `Key`, so without its own event the state would still read whatever the last typed character carried.
#[test]
fn a_bare_modifier_is_visible_without_any_key_event() {
    fresh();
    assert!(!modifiers().is_shift, "shift is not down to begin with");
    observe(&Event::ModifiersChanged { modifiers: shift() });
    assert!(
        modifiers().is_shift,
        "a modifier-only event still updates the state"
    );
}

/// Losing focus mid-chord is exactly where a reconstructed state goes wrong: the releases never come.
#[test]
fn losing_focus_forgets_what_was_held() {
    fresh();
    observe(&Event::KeyPressed {
        key: up(),
        modifiers: shift(),
    });
    assert!(
        key_held(&up()) && modifiers().is_shift,
        "the key and the modifier are both live before focus goes"
    );
    observe(&Event::FocusChanged { is_focused: false });
    assert!(!key_held(&up()), "losing focus releases the key");
    assert!(!modifiers().is_shift, "and clears the modifier with it");
}
