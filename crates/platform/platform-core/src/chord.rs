//! [`Chord`]: a keyboard shortcut written once, shown and matched the way the platform it runs on spells it.

use std::cell::Cell;
use std::fmt;
use std::str::FromStr;

use crate::{Key, ModifiersState, NamedKey};

/// Which platform's keyboard a shortcut is shown and matched for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeyboardConvention {
    /// macOS and iPadOS: Command is the primary modifier, and a chord is shown as symbols, `⇧⌘Z`.
    Mac,
    /// Everything else: Ctrl is the primary modifier, and a chord is shown as words, `Ctrl+Shift+Z`.
    Pc,
}

impl KeyboardConvention {
    /// The convention of the operating system this was compiled for.
    pub const fn native() -> Self {
        if cfg!(any(target_os = "macos", target_os = "ios")) {
            Self::Mac
        } else {
            Self::Pc
        }
    }

    /// The convention of the system a browser names, as `navigator.platform` (`MacIntel`, `Win32`, `iPhone`) or `navigator.userAgentData.platform` (`macOS`, `Windows`) spell it.
    pub fn of_platform_name(name: &str) -> Self {
        let name = name.trim().to_ascii_lowercase();
        let apple = name.contains("mac")
            || ["iphone", "ipad", "ipod", "ios"]
                .iter()
                .any(|device| name.starts_with(device));
        if apple { Self::Mac } else { Self::Pc }
    }

    /// The modifier that stands for the primary one, `Mod` in a chord's text: Command on a Mac, Ctrl elsewhere.
    pub const fn primary(self) -> ModifiersState {
        ModifiersState {
            is_shift: false,
            is_ctrl: matches!(self, Self::Pc),
            is_alt: false,
            is_meta: matches!(self, Self::Mac),
        }
    }
}

reactive_local::dropless_thread_local! {
    static CONVENTION: Cell<Option<KeyboardConvention>> = const { Cell::new(None) };
}

/// The convention shortcuts are shown and matched in on this thread: what the running platform installed, else [`KeyboardConvention::native`].
pub fn keyboard_convention() -> KeyboardConvention {
    CONVENTION
        .with(Cell::get)
        .unwrap_or(KeyboardConvention::native())
}

/// Installs the convention [`keyboard_convention`] answers with. The runner installs what its platform reports before it builds the app; a test installs one to pin what a shortcut shows.
pub fn set_keyboard_convention(convention: KeyboardConvention) {
    CONVENTION.with(|current| current.set(Some(convention)));
}

impl ModifiersState {
    /// Whether the primary modifier of the convention in force is held: Command on a Mac, Ctrl elsewhere.
    pub fn is_primary(&self) -> bool {
        match keyboard_convention() {
            KeyboardConvention::Mac => self.is_meta,
            KeyboardConvention::Pc => self.is_ctrl,
        }
    }
}

/// A key and the modifiers held with it, written portably: `"Mod+Z"` is Command+Z on a Mac and Ctrl+Z elsewhere.
///
/// Parsed from text such as `"Mod+Shift+Z"`, `"Ctrl+K"`, `"Alt+↑"`, `"F5"` or `"⇧⌘Z"`. `Mod` is the primary modifier of the convention in force; `Ctrl`, `Cmd` (`Meta`, `Super`), `Alt` (`Option`) and `Shift` name one key everywhere. A key that is itself `+` is written last: `"Ctrl++"`. [`Display`](fmt::Display) writes it the way the platform does.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Chord {
    key: Key,
    primary: bool,
    ctrl: bool,
    alt: bool,
    shift: bool,
    meta: bool,
}

impl Chord {
    /// `key` with no modifier held.
    pub fn new(key: Key) -> Self {
        Self {
            key: fold(key),
            primary: false,
            ctrl: false,
            alt: false,
            shift: false,
            meta: false,
        }
    }

    pub fn with_primary(self) -> Self {
        Self {
            primary: true,
            ..self
        }
    }

    pub fn with_ctrl(self) -> Self {
        Self { ctrl: true, ..self }
    }

    pub fn with_alt(self) -> Self {
        Self { alt: true, ..self }
    }

    pub fn with_shift(self) -> Self {
        Self {
            shift: true,
            ..self
        }
    }

    pub fn with_meta(self) -> Self {
        Self { meta: true, ..self }
    }

    pub fn parse(text: &str) -> Result<Self, ChordError> {
        let tokens = tokens(text);
        let (last, held) = tokens.split_last().ok_or(ChordError::Empty)?;
        let glyphs_end: usize = last
            .chars()
            .take_while(|c| modifier_glyph(*c).is_some())
            .map(char::len_utf8)
            .sum();
        let (glyphs, key) = last.split_at(glyphs_end);
        if key.is_empty() {
            return Err(ChordError::NoKey(text.trim().to_string()));
        }
        let key = key_named(key).ok_or_else(|| ChordError::UnknownKey(key.to_string()))?;
        let mut chord = Self::new(key);
        for token in held {
            chord = chord.holding(token)?;
        }
        if !glyphs.is_empty() {
            chord = chord.holding(glyphs)?;
        }
        Ok(chord)
    }

    fn holding(self, token: &str) -> Result<Self, ChordError> {
        if let Some(held) = modifier_named(token) {
            return Ok(held.apply(self));
        }
        let glyphs: Option<Vec<Modifier>> = token.chars().map(modifier_glyph).collect();
        match glyphs {
            Some(glyphs) if !glyphs.is_empty() => Ok(glyphs
                .into_iter()
                .fold(self, |chord, held| held.apply(chord))),
            _ => Err(ChordError::UnknownModifier(token.to_string())),
        }
    }

    pub fn key(&self) -> &Key {
        &self.key
    }

    /// The modifiers the chord holds under `convention`, its primary one resolved.
    pub fn modifiers(&self, convention: KeyboardConvention) -> ModifiersState {
        let primary = convention.primary();
        ModifiersState {
            is_shift: self.shift,
            is_ctrl: self.ctrl || (self.primary && primary.is_ctrl),
            is_alt: self.alt,
            is_meta: self.meta || (self.primary && primary.is_meta),
        }
    }

    /// Whether a press of `key` with `modifiers` held is this chord, under the convention in force.
    ///
    /// Ctrl, Alt and Command must be exactly the chord's. A letter matches in either case and Shift must be exactly the chord's; any other character is matched as typed, so `?` matches the Shift that typed it, unless the chord names Shift itself. A press whose character Alt composed (Option+T is `†` on a Mac) is matched on the key it makes with no modifier, which is what `shortcut_key` answers.
    pub fn matches(&self, key: &Key, modifiers: ModifiersState) -> bool {
        self.matches_in(key, modifiers, keyboard_convention())
    }

    pub fn matches_in(
        &self,
        key: &Key,
        modifiers: ModifiersState,
        convention: KeyboardConvention,
    ) -> bool {
        let wanted = self.modifiers(convention);
        if (wanted.is_ctrl, wanted.is_alt, wanted.is_meta)
            != (modifiers.is_ctrl, modifiers.is_alt, modifiers.is_meta)
        {
            return false;
        }
        let shift_counts = match &self.key {
            Key::Char(c) => c.is_alphabetic() || self.shift,
            Key::Named(_) => true,
        };
        (!shift_counts || wanted.is_shift == modifiers.is_shift) && fold(key.clone()) == self.key
    }

    /// The chord as `convention` shows it, one entry per key: `["Ctrl", "Shift", "Z"]`, or `["⇧", "⌘", "Z"]` on a Mac.
    pub fn keys(&self, convention: KeyboardConvention) -> Vec<String> {
        let held = self.modifiers(convention);
        let names: [(bool, &str, &str); 4] = [
            (held.is_ctrl, "⌃", "Ctrl"),
            (held.is_alt, "⌥", "Alt"),
            (held.is_shift, "⇧", "Shift"),
            (held.is_meta, "⌘", "Meta"),
        ];
        names
            .into_iter()
            .filter(|(held, ..)| *held)
            .map(|(_, mac, pc)| match convention {
                KeyboardConvention::Mac => mac.to_string(),
                KeyboardConvention::Pc => pc.to_string(),
            })
            .chain([key_label(&self.key, convention)])
            .collect()
    }

    /// The chord as `convention` writes it: `Ctrl+Shift+Z`, or `⇧⌘Z` on a Mac.
    pub fn text(&self, convention: KeyboardConvention) -> String {
        let separator = match convention {
            KeyboardConvention::Mac => "",
            KeyboardConvention::Pc => "+",
        };
        self.keys(convention).join(separator)
    }

    /// The chord in `aria-keyshortcuts` syntax: modifiers first, each spelled as UI Events names its key (`Control`, `Meta`), so a Mac gets `Meta+Shift+Z` where a PC gets `Control+Shift+Z`.
    pub fn aria(&self, convention: KeyboardConvention) -> String {
        let held = self.modifiers(convention);
        let modifiers: [(bool, &str); 4] = [
            (held.is_ctrl, "Control"),
            (held.is_alt, "Alt"),
            (held.is_shift, "Shift"),
            (held.is_meta, "Meta"),
        ];
        modifiers
            .into_iter()
            .filter(|(held, _)| *held)
            .map(|(_, name)| name.to_string())
            .chain([aria_key(&self.key)])
            .collect::<Vec<_>>()
            .join("+")
    }
}

impl fmt::Display for Chord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text(keyboard_convention()))
    }
}

impl FromStr for Chord {
    type Err = ChordError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::parse(text)
    }
}

/// Why text names no [`Chord`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChordError {
    Empty,
    NoKey(String),
    UnknownModifier(String),
    UnknownKey(String),
}

impl fmt::Display for ChordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("a chord names at least a key"),
            Self::NoKey(chord) => write!(f, "`{chord}` holds modifiers but names no key"),
            Self::UnknownModifier(name) => write!(
                f,
                "`{name}` is not a modifier: Mod, Ctrl, Cmd, Alt or Shift"
            ),
            Self::UnknownKey(name) => write!(f, "`{name}` is not a key"),
        }
    }
}

impl std::error::Error for ChordError {}

#[derive(Clone, Copy)]
enum Modifier {
    Primary,
    Ctrl,
    Alt,
    Shift,
    Meta,
}

impl Modifier {
    fn apply(self, chord: Chord) -> Chord {
        match self {
            Self::Primary => chord.with_primary(),
            Self::Ctrl => chord.with_ctrl(),
            Self::Alt => chord.with_alt(),
            Self::Shift => chord.with_shift(),
            Self::Meta => chord.with_meta(),
        }
    }
}

fn modifier_named(name: &str) -> Option<Modifier> {
    let modifier = match name.to_ascii_lowercase().as_str() {
        "mod" | "primary" | "cmdorctrl" | "commandorcontrol" => Modifier::Primary,
        "ctrl" | "control" => Modifier::Ctrl,
        "alt" | "option" | "opt" => Modifier::Alt,
        "shift" => Modifier::Shift,
        "cmd" | "command" | "meta" | "super" | "win" => Modifier::Meta,
        _ => return None,
    };
    Some(modifier)
}

fn modifier_glyph(c: char) -> Option<Modifier> {
    match c {
        '⌃' => Some(Modifier::Ctrl),
        '⌥' => Some(Modifier::Alt),
        '⇧' => Some(Modifier::Shift),
        '⌘' => Some(Modifier::Meta),
        _ => None,
    }
}

/// The parts of a chord's text, split on `+` except where `+` is the key itself.
fn tokens(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for c in text.trim().chars() {
        if c == '+' && !current.trim().is_empty() {
            tokens.push(std::mem::take(&mut current));
        } else {
            current.push(c);
        }
    }
    tokens.push(current);
    tokens
        .into_iter()
        .map(|token| token.trim().to_string())
        .filter(|token| !token.is_empty())
        .collect()
}

const KEY_NAMES: &[(&str, NamedKey)] = &[
    ("enter", NamedKey::Enter),
    ("return", NamedKey::Enter),
    ("⏎", NamedKey::Enter),
    ("↩", NamedKey::Enter),
    ("escape", NamedKey::Escape),
    ("esc", NamedKey::Escape),
    ("⎋", NamedKey::Escape),
    ("tab", NamedKey::Tab),
    ("⇥", NamedKey::Tab),
    ("backspace", NamedKey::Backspace),
    ("⌫", NamedKey::Backspace),
    ("delete", NamedKey::Delete),
    ("del", NamedKey::Delete),
    ("⌦", NamedKey::Delete),
    ("insert", NamedKey::Insert),
    ("ins", NamedKey::Insert),
    ("home", NamedKey::Home),
    ("↖", NamedKey::Home),
    ("end", NamedKey::End),
    ("↘", NamedKey::End),
    ("pageup", NamedKey::PageUp),
    ("pgup", NamedKey::PageUp),
    ("⇞", NamedKey::PageUp),
    ("pagedown", NamedKey::PageDown),
    ("pgdn", NamedKey::PageDown),
    ("⇟", NamedKey::PageDown),
    ("up", NamedKey::ArrowUp),
    ("arrowup", NamedKey::ArrowUp),
    ("↑", NamedKey::ArrowUp),
    ("down", NamedKey::ArrowDown),
    ("arrowdown", NamedKey::ArrowDown),
    ("↓", NamedKey::ArrowDown),
    ("left", NamedKey::ArrowLeft),
    ("arrowleft", NamedKey::ArrowLeft),
    ("←", NamedKey::ArrowLeft),
    ("right", NamedKey::ArrowRight),
    ("arrowright", NamedKey::ArrowRight),
    ("→", NamedKey::ArrowRight),
    ("space", NamedKey::Space),
    ("capslock", NamedKey::CapsLock),
    ("⇪", NamedKey::CapsLock),
    ("menu", NamedKey::ContextMenu),
    ("contextmenu", NamedKey::ContextMenu),
    ("num0", NamedKey::Numpad0),
    ("num1", NamedKey::Numpad1),
    ("num2", NamedKey::Numpad2),
    ("num3", NamedKey::Numpad3),
    ("num4", NamedKey::Numpad4),
    ("num5", NamedKey::Numpad5),
    ("num6", NamedKey::Numpad6),
    ("num7", NamedKey::Numpad7),
    ("num8", NamedKey::Numpad8),
    ("num9", NamedKey::Numpad9),
    ("numadd", NamedKey::NumpadAdd),
    ("numpadadd", NamedKey::NumpadAdd),
    ("numsubtract", NamedKey::NumpadSubtract),
    ("numpadsubtract", NamedKey::NumpadSubtract),
    ("nummultiply", NamedKey::NumpadMultiply),
    ("numpadmultiply", NamedKey::NumpadMultiply),
    ("numdivide", NamedKey::NumpadDivide),
    ("numpaddivide", NamedKey::NumpadDivide),
    ("numdecimal", NamedKey::NumpadDecimal),
    ("numpaddecimal", NamedKey::NumpadDecimal),
    ("numenter", NamedKey::NumpadEnter),
    ("numpadenter", NamedKey::NumpadEnter),
    ("num⏎", NamedKey::NumpadEnter),
];

const FUNCTION_KEYS: [NamedKey; 24] = [
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

fn key_named(name: &str) -> Option<Key> {
    let lower = name.to_lowercase();
    if let Some((_, named)) = KEY_NAMES.iter().find(|(alias, _)| *alias == lower) {
        return Some(Key::Named(named.clone()));
    }
    let function = lower
        .strip_prefix('f')
        .and_then(|number| number.parse::<usize>().ok())
        .and_then(|number| FUNCTION_KEYS.get(number.checked_sub(1)?));
    if let Some(named) = function {
        return Some(Key::Named(named.clone()));
    }
    let mut chars = name.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => Some(Key::Char(c)),
        _ => None,
    }
}

fn key_label(key: &Key, convention: KeyboardConvention) -> String {
    match key {
        Key::Char(' ') => "Space".to_string(),
        Key::Char(c) => c.to_uppercase().collect(),
        Key::Named(named) => named_label(named, convention),
    }
}

fn named_label(named: &NamedKey, convention: KeyboardConvention) -> String {
    let (mac, pc) = match named {
        NamedKey::Enter => ("⏎", "Enter"),
        NamedKey::Escape => ("⎋", "Esc"),
        NamedKey::Tab => ("⇥", "Tab"),
        NamedKey::Backspace => ("⌫", "Backspace"),
        NamedKey::Delete => ("⌦", "Delete"),
        NamedKey::Insert => ("Insert", "Insert"),
        NamedKey::Home => ("↖", "Home"),
        NamedKey::End => ("↘", "End"),
        NamedKey::PageUp => ("⇞", "PageUp"),
        NamedKey::PageDown => ("⇟", "PageDown"),
        NamedKey::ArrowUp => ("↑", "↑"),
        NamedKey::ArrowDown => ("↓", "↓"),
        NamedKey::ArrowLeft => ("←", "←"),
        NamedKey::ArrowRight => ("→", "→"),
        NamedKey::Space => ("Space", "Space"),
        NamedKey::CapsLock => ("⇪", "CapsLock"),
        NamedKey::ContextMenu => ("Menu", "Menu"),
        NamedKey::Numpad0 => ("Num0", "Num0"),
        NamedKey::Numpad1 => ("Num1", "Num1"),
        NamedKey::Numpad2 => ("Num2", "Num2"),
        NamedKey::Numpad3 => ("Num3", "Num3"),
        NamedKey::Numpad4 => ("Num4", "Num4"),
        NamedKey::Numpad5 => ("Num5", "Num5"),
        NamedKey::Numpad6 => ("Num6", "Num6"),
        NamedKey::Numpad7 => ("Num7", "Num7"),
        NamedKey::Numpad8 => ("Num8", "Num8"),
        NamedKey::Numpad9 => ("Num9", "Num9"),
        NamedKey::NumpadAdd => ("NumAdd", "NumAdd"),
        NamedKey::NumpadSubtract => ("NumSubtract", "NumSubtract"),
        NamedKey::NumpadMultiply => ("NumMultiply", "NumMultiply"),
        NamedKey::NumpadDivide => ("NumDivide", "NumDivide"),
        NamedKey::NumpadDecimal => ("NumDecimal", "NumDecimal"),
        NamedKey::NumpadEnter => ("Num⏎", "NumEnter"),
        NamedKey::F1
        | NamedKey::F2
        | NamedKey::F3
        | NamedKey::F4
        | NamedKey::F5
        | NamedKey::F6
        | NamedKey::F7
        | NamedKey::F8
        | NamedKey::F9
        | NamedKey::F10
        | NamedKey::F11
        | NamedKey::F12
        | NamedKey::F13
        | NamedKey::F14
        | NamedKey::F15
        | NamedKey::F16
        | NamedKey::F17
        | NamedKey::F18
        | NamedKey::F19
        | NamedKey::F20
        | NamedKey::F21
        | NamedKey::F22
        | NamedKey::F23
        | NamedKey::F24 => return format!("{named:?}"),
    };
    match convention {
        KeyboardConvention::Mac => mac.to_string(),
        KeyboardConvention::Pc => pc.to_string(),
    }
}

/// The key as `aria-keyshortcuts` writes it: a character in capitals, `Space` and `Plus` spelled out because the grammar reserves their characters, and a keypad key as the character it types.
fn aria_key(key: &Key) -> String {
    let typed = match key {
        Key::Char(' ') | Key::Named(NamedKey::Space) => "Space",
        Key::Char('+') | Key::Named(NamedKey::NumpadAdd) => "Plus",
        Key::Named(NamedKey::NumpadEnter) => "Enter",
        Key::Named(NamedKey::NumpadSubtract) => "-",
        Key::Named(NamedKey::NumpadMultiply) => "*",
        Key::Named(NamedKey::NumpadDivide) => "/",
        Key::Named(NamedKey::NumpadDecimal) => ".",
        Key::Named(NamedKey::Numpad0) => "0",
        Key::Named(NamedKey::Numpad1) => "1",
        Key::Named(NamedKey::Numpad2) => "2",
        Key::Named(NamedKey::Numpad3) => "3",
        Key::Named(NamedKey::Numpad4) => "4",
        Key::Named(NamedKey::Numpad5) => "5",
        Key::Named(NamedKey::Numpad6) => "6",
        Key::Named(NamedKey::Numpad7) => "7",
        Key::Named(NamedKey::Numpad8) => "8",
        Key::Named(NamedKey::Numpad9) => "9",
        Key::Char(c) => return c.to_uppercase().collect(),
        Key::Named(named) => return format!("{named:?}"),
    };
    typed.to_string()
}

/// A letter in one case, so a chord written `K` and a press reporting `k` are the same key.
fn fold(key: Key) -> Key {
    match key {
        Key::Char(c) => {
            let mut lower = c.to_lowercase();
            match (lower.next(), lower.next()) {
                (Some(folded), None) => Key::Char(folded),
                _ => Key::Char(c),
            }
        }
        named => named,
    }
}

#[cfg(test)]
#[path = "chord_test.rs"]
mod tests;
