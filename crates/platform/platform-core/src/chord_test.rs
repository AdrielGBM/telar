use super::*;
use KeyboardConvention::{Mac, Pc};

fn chord(text: &str) -> Chord {
    Chord::parse(text).unwrap_or_else(|e| panic!("{text}: {e}"))
}

fn held(ctrl: bool, alt: bool, shift: bool, meta: bool) -> ModifiersState {
    ModifiersState {
        is_shift: shift,
        is_ctrl: ctrl,
        is_alt: alt,
        is_meta: meta,
    }
}

const NONE: ModifiersState = ModifiersState {
    is_shift: false,
    is_ctrl: false,
    is_alt: false,
    is_meta: false,
};

#[test]
fn a_portable_chord_reads_its_modifiers_and_its_key() {
    assert_eq!(
        chord("Mod+Shift+Z"),
        Chord::new(Key::Char('z')).with_primary().with_shift()
    );
    assert_eq!(chord("Ctrl+K"), Chord::new(Key::Char('k')).with_ctrl());
    assert_eq!(chord("cmd+option+p"), chord("Meta+Alt+P"));
    assert_eq!(
        chord("Alt+↑"),
        Chord::new(Key::Named(NamedKey::ArrowUp)).with_alt()
    );
    assert_eq!(chord("F5"), Chord::new(Key::Named(NamedKey::F5)));
    assert_eq!(chord("Shift+F12"), chord("shift+f12"));
    assert_eq!(chord(" Ctrl + K "), chord("Ctrl+K"));
}

#[test]
fn a_key_that_is_a_plus_is_written_last() {
    assert_eq!(chord("Ctrl++"), Chord::new(Key::Char('+')).with_ctrl());
    assert_eq!(chord("+"), Chord::new(Key::Char('+')));
    assert_eq!(chord("Alt+="), Chord::new(Key::Char('=')).with_alt());
}

#[test]
fn the_symbols_a_mac_shows_read_back_as_the_keys_they_stand_for() {
    assert_eq!(
        chord("⇧⌘Z"),
        Chord::new(Key::Char('z')).with_shift().with_meta()
    );
    assert_eq!(chord("⌃⌥Delete"), chord("Ctrl+Alt+Delete"));
    assert_eq!(chord("⌘+K"), chord("Cmd+K"));
    assert_eq!(chord("⌘⏎"), chord("Cmd+Enter"));
}

#[test]
fn text_that_names_no_chord_says_why() {
    assert_eq!(Chord::parse("  "), Err(ChordError::Empty));
    assert_eq!(Chord::parse("⌘"), Err(ChordError::NoKey("⌘".into())));
    assert_eq!(
        Chord::parse("Hyper+K"),
        Err(ChordError::UnknownModifier("Hyper".into()))
    );
    assert_eq!(
        Chord::parse("Ctrl+Kay"),
        Err(ChordError::UnknownKey("Kay".into()))
    );
    assert!("Mod+Z".parse::<Chord>().is_ok());
}

#[test]
fn every_named_key_reads_back_from_how_it_is_shown() {
    let named = [
        NamedKey::Enter,
        NamedKey::Backspace,
        NamedKey::Escape,
        NamedKey::Tab,
        NamedKey::Delete,
        NamedKey::Home,
        NamedKey::End,
        NamedKey::PageUp,
        NamedKey::PageDown,
        NamedKey::ArrowUp,
        NamedKey::ArrowDown,
        NamedKey::ArrowLeft,
        NamedKey::ArrowRight,
        NamedKey::Space,
        NamedKey::Insert,
        NamedKey::CapsLock,
        NamedKey::ContextMenu,
        NamedKey::Numpad0,
        NamedKey::Numpad9,
        NamedKey::NumpadAdd,
        NamedKey::NumpadSubtract,
        NamedKey::NumpadMultiply,
        NamedKey::NumpadDivide,
        NamedKey::NumpadDecimal,
        NamedKey::NumpadEnter,
    ]
    .into_iter()
    .chain(FUNCTION_KEYS);
    for key in named {
        let shortcut = Chord::new(Key::Named(key)).with_primary();
        for convention in [Mac, Pc] {
            let shown = shortcut.text(convention);
            assert_eq!(
                Chord::parse(&shown).map(|read| read.modifiers(convention)),
                Ok(shortcut.modifiers(convention)),
                "{shown}"
            );
            assert_eq!(
                Chord::parse(&shown).unwrap().key(),
                shortcut.key(),
                "{shown}"
            );
        }
    }
}

#[test]
fn a_mac_shows_symbols_in_its_own_order_and_a_pc_shows_words() {
    let redo = chord("Mod+Shift+Z");
    assert_eq!(redo.text(Mac), "⇧⌘Z");
    assert_eq!(redo.text(Pc), "Ctrl+Shift+Z");
    assert_eq!(redo.keys(Mac), ["⇧", "⌘", "Z"]);
    assert_eq!(redo.keys(Pc), ["Ctrl", "Shift", "Z"]);

    let every = chord("Shift+Cmd+Alt+Ctrl+K");
    assert_eq!(every.text(Mac), "⌃⌥⇧⌘K");
    assert_eq!(every.text(Pc), "Ctrl+Alt+Shift+Meta+K");

    assert_eq!(
        chord("Ctrl+K").text(Mac),
        "⌃K",
        "an explicit Ctrl stays Ctrl"
    );
    assert_eq!(chord("Alt+↑").text(Mac), "⌥↑");
    assert_eq!(chord("Alt+↑").text(Pc), "Alt+↑");
    assert_eq!(chord("Mod+Enter").text(Mac), "⌘⏎");
    assert_eq!(chord("Mod+Enter").text(Pc), "Ctrl+Enter");
    assert_eq!(chord("Ctrl++").text(Pc), "Ctrl++");
    assert_eq!(chord("Shift+F5").text(Mac), "⇧F5");
    assert_eq!(chord("?").text(Pc), "?");
}

#[test]
fn display_follows_the_convention_installed_on_this_thread() {
    let undo = chord("Mod+Z");
    set_keyboard_convention(Pc);
    assert_eq!(undo.to_string(), "Ctrl+Z");
    set_keyboard_convention(Mac);
    assert_eq!(undo.to_string(), "⌘Z");
    assert!(
        std::thread::spawn(
            move || undo.to_string() == chord("Mod+Z").text(KeyboardConvention::native())
        )
        .join()
        .unwrap(),
        "another thread keeps the native convention"
    );
}

#[test]
fn mod_matches_command_on_a_mac_and_ctrl_elsewhere() {
    let undo = chord("Mod+Z");
    let z = Key::Char('z');
    assert!(undo.matches_in(&z, held(false, false, false, true), Mac));
    assert!(!undo.matches_in(&z, held(true, false, false, false), Mac));
    assert!(undo.matches_in(&z, held(true, false, false, false), Pc));
    assert!(!undo.matches_in(&z, held(false, false, false, true), Pc));
    assert!(!undo.matches_in(&z, NONE, Pc));
}

#[test]
fn a_letter_matches_in_either_case_with_shift_exactly_as_written() {
    let redo = chord("Mod+Shift+Z");
    let undo = chord("Mod+Z");
    let shifted = held(true, false, true, false);
    assert!(redo.matches_in(&Key::Char('Z'), shifted, Pc));
    assert!(redo.matches_in(&Key::Char('z'), shifted, Pc));
    assert!(!undo.matches_in(&Key::Char('Z'), shifted, Pc));
    assert!(
        undo.matches_in(&Key::Char('Z'), held(true, false, false, false), Pc),
        "Caps Lock"
    );
}

#[test]
fn a_symbol_matches_whatever_shift_it_took_to_type_it() {
    assert!(chord("?").matches_in(&Key::Char('?'), held(false, false, true, false), Pc));
    assert!(chord("?").matches_in(&Key::Char('?'), NONE, Pc));
    assert!(!chord("/").matches_in(&Key::Char('?'), held(false, false, true, false), Pc));
    assert!(chord("Alt+=").matches_in(&Key::Char('='), held(false, true, true, false), Mac));
    assert!(!chord("Alt+Shift+1").matches_in(&Key::Char('1'), held(false, true, false, false), Pc));
}

#[test]
fn ctrl_alt_and_command_are_never_extra() {
    let find = chord("Mod+K");
    assert!(!find.matches_in(&Key::Char('k'), held(true, true, false, false), Pc));
    assert!(!chord("Alt+S").matches_in(&Key::Char('s'), held(true, true, false, false), Pc));
    assert!(!chord("K").matches_in(&Key::Char('k'), held(false, false, false, true), Mac));
}

#[test]
fn a_named_key_matches_with_shift_exactly_as_written() {
    let up = chord("Alt+↑");
    let arrow = Key::Named(NamedKey::ArrowUp);
    assert!(up.matches_in(&arrow, held(false, true, false, false), Pc));
    assert!(!up.matches_in(&arrow, held(false, true, true, false), Pc));
    assert!(!up.matches_in(
        &Key::Named(NamedKey::ArrowDown),
        held(false, true, false, false),
        Pc
    ));
}

#[test]
fn the_primary_modifier_held_is_the_conventions_own() {
    set_keyboard_convention(Pc);
    assert!(held(true, false, false, false).is_primary());
    assert!(!held(false, false, false, true).is_primary());
    set_keyboard_convention(Mac);
    assert!(held(false, false, false, true).is_primary());
    assert!(!held(true, false, false, false).is_primary());
}

#[test]
fn a_browser_on_an_apple_system_has_a_mac_keyboard() {
    for name in [
        "MacIntel",
        "macOS",
        "Macintosh",
        "iPhone",
        "iPad",
        "iPod touch",
    ] {
        assert_eq!(KeyboardConvention::of_platform_name(name), Mac, "{name}");
    }
    for name in [
        "Win32",
        "Windows",
        "Linux x86_64",
        "Android",
        "Chrome OS",
        "",
    ] {
        assert_eq!(KeyboardConvention::of_platform_name(name), Pc, "{name}");
    }
}

#[test]
fn the_aria_form_names_keys_as_ui_events_does_and_resolves_mod_per_platform() {
    let redo = chord("Mod+Shift+Z");
    assert_eq!(redo.aria(Pc), "Control+Shift+Z");
    assert_eq!(redo.aria(Mac), "Shift+Meta+Z");

    let every = chord("Meta+Alt+Shift+Ctrl+k");
    assert_eq!(every.aria(Pc), "Control+Alt+Shift+Meta+K");
    assert_eq!(every.aria(Mac), every.aria(Pc));

    assert_eq!(chord("Mod+Enter").aria(Pc), "Control+Enter");
    assert_eq!(chord("Alt+↑").aria(Mac), "Alt+ArrowUp");
    assert_eq!(chord("PageDown").aria(Pc), "PageDown");
    assert_eq!(chord("Shift+F5").aria(Pc), "Shift+F5");
    assert_eq!(chord("Mod+Space").aria(Pc), "Control+Space");
    assert_eq!(chord("Ctrl++").aria(Pc), "Control+Plus");
    assert_eq!(chord("Ctrl+NumAdd").aria(Pc), "Control+Plus");
    assert_eq!(chord("Ctrl+Num7").aria(Pc), "Control+7");
    assert_eq!(chord("Menu").aria(Pc), "ContextMenu");
    assert_eq!(chord("Esc").aria(Pc), "Escape");
}
