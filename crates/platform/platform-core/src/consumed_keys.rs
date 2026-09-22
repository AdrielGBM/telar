//! Reading a focused box's [`ConsumedKeys`] against the keys a backend actually receives.

use crate::{Key, NamedKey};

pub use semantics_core::ConsumedKeys;

/// The attribute a document backend writes a focusable box's consumed keys to, and the attribute its platform reads them from inside the key listener, where waiting a frame for the app would be too late to keep the host's default action from running.
pub const CONSUMED_KEYS_ATTRIBUTE: &str = "data-telar-keys";

/// The attribute a document backend writes a focusable box's identity to, so a focus move the host made on its own can be reported as the box it landed on.
pub const FOCUS_BOX_ATTRIBUTE: &str = "data-telar-focus";

/// The member of [`ConsumedKeys`] that stands for `key`; empty for a key no host acts on by default.
pub fn key_member(key: &Key) -> ConsumedKeys {
    let Key::Named(named) = key else {
        return ConsumedKeys::EMPTY;
    };
    match named {
        NamedKey::Tab => ConsumedKeys::TAB,
        NamedKey::Space => ConsumedKeys::SPACE,
        NamedKey::Enter | NamedKey::NumpadEnter => ConsumedKeys::ENTER,
        NamedKey::Backspace => ConsumedKeys::BACKSPACE,
        NamedKey::ArrowUp => ConsumedKeys::ARROW_UP,
        NamedKey::ArrowDown => ConsumedKeys::ARROW_DOWN,
        NamedKey::ArrowLeft => ConsumedKeys::ARROW_LEFT,
        NamedKey::ArrowRight => ConsumedKeys::ARROW_RIGHT,
        NamedKey::PageUp => ConsumedKeys::PAGE_UP,
        NamedKey::PageDown => ConsumedKeys::PAGE_DOWN,
        NamedKey::Home => ConsumedKeys::HOME,
        NamedKey::End => ConsumedKeys::END,
        _ => ConsumedKeys::EMPTY,
    }
}

/// Whether a box declaring `keys` keeps `key` for itself.
pub fn consumes(keys: ConsumedKeys, key: &Key) -> bool {
    keys.intersects(key_member(key))
}

#[cfg(test)]
#[path = "consumed_keys_test.rs"]
mod tests;
