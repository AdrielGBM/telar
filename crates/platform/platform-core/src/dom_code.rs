//! [`unmodified_key_of_code`]: the key a DOM `KeyboardEvent.code` names, for a browser that composes characters from the modifiers.

use crate::Key;

/// The key a `KeyboardEvent.code` stands for on a US layout, which no modifier changes.
///
/// A browser reports only the composed `key`, so Option+T on macOS arrives as `†`; `code` is the one reading that does not move with the modifiers. It names a position rather than what the user's layout prints there, so a backend uses it only where `key` has already left ASCII and carries nothing a shortcut could match. Only the keys that type a character are named here; a named key such as an arrow is never composed.
pub fn unmodified_key_of_code(code: &str) -> Option<Key> {
    let character = match code {
        "Minus" => '-',
        "Equal" => '=',
        "BracketLeft" => '[',
        "BracketRight" => ']',
        "Backslash" => '\\',
        "Semicolon" => ';',
        "Quote" => '\'',
        "Backquote" => '`',
        "Comma" => ',',
        "Period" => '.',
        "Slash" => '/',
        _ => return lettered_or_digit(code),
    };
    Some(Key::Char(character))
}

fn lettered_or_digit(code: &str) -> Option<Key> {
    let name = code
        .strip_prefix("Key")
        .or_else(|| code.strip_prefix("Digit"))?;
    let mut chars = name.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if c.is_ascii_alphanumeric() => Some(Key::Char(c.to_ascii_lowercase())),
        _ => None,
    }
}

#[cfg(test)]
#[path = "dom_code_test.rs"]
mod tests;
