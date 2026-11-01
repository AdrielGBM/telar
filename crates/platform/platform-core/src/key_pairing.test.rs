use super::*;

const BACKSLASH_KEY: u32 = 41;

/// The case that leaves a key held for ever: `AltGr` let go before the key it modified, so the layout reads the release as the key's plain level.
#[test]
fn a_release_names_what_its_press_produced_whatever_the_layout_reads_now() {
    let mut pairing = KeyPairing::default();
    assert_eq!(
        pairing.press(BACKSLASH_KEY, Key::Char('\\')),
        Key::Char('\\')
    );
    assert_eq!(
        pairing.release(&BACKSLASH_KEY, Some(Key::Char('º'))),
        Some(Key::Char('\\'))
    );
}

/// A layout can read the release as no key at all — a dead key once the modifier is gone — and the press still needs its release.
#[test]
fn a_release_the_layout_cannot_name_still_names_its_press() {
    let mut pairing = KeyPairing::default();
    pairing.press(BACKSLASH_KEY, Key::Char('\\'));
    assert_eq!(pairing.release(&BACKSLASH_KEY, None), Some(Key::Char('\\')));
}

#[test]
fn a_repeat_reports_the_key_its_hold_began_as() {
    let mut pairing = KeyPairing::default();
    pairing.press(BACKSLASH_KEY, Key::Char('\\'));
    assert_eq!(
        pairing.press(BACKSLASH_KEY, Key::Char('º')),
        Key::Char('\\')
    );
    assert_eq!(
        pairing.release(&BACKSLASH_KEY, Some(Key::Char('º'))),
        Some(Key::Char('\\'))
    );
}

#[test]
fn a_release_whose_press_was_never_seen_keeps_the_layouts_reading() {
    let mut pairing = KeyPairing::default();
    assert_eq!(
        pairing.release(&7, Some(Key::Char('a'))),
        Some(Key::Char('a'))
    );
    assert_eq!(pairing.release(&7, None), None);
}

#[test]
fn a_cleared_pairing_has_nothing_down() {
    let mut pairing = KeyPairing::default();
    pairing.press(BACKSLASH_KEY, Key::Char('\\'));
    pairing.clear();
    assert_eq!(
        pairing.release(&BACKSLASH_KEY, Some(Key::Char('º'))),
        Some(Key::Char('º'))
    );
}
