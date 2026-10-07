//! [`KeyPairing`]: the key a release names, taken from the press the same physical key made.

use crate::Key;

/// Remembers which [`Key`] each physical key produced as it went down, so its release names that key and not whatever the layout would make of it now.
///
/// A layout maps a physical key through the modifiers held at the moment it asks. With a level-three shift (`AltGr`) let go before the key it modified, the release maps to the key's plain level — `º` where the press was `\` — and anything that pairs releases with presses by [`Key`], as `key_held` does, keeps the `\` down for ever. The physical key is the one thing a press and its release always share, so a backend that has one (a keycode, a scancode, a DOM `code`) reports both halves through this and the pair stays a pair.
///
/// `C` is whatever the backend identifies a physical key by. A handful of keys are down at once, so the record is a short list rather than a map.
#[derive(Debug, Clone)]
pub struct KeyPairing<C> {
    down: Vec<(C, Key)>,
}

impl<C> Default for KeyPairing<C> {
    fn default() -> Self {
        Self { down: Vec::new() }
    }
}

impl<C: PartialEq> KeyPairing<C> {
    /// The key to report for `code` going down, which the layout mapped to `key`.
    ///
    /// A repeat of a key already down reports what its first press did: one hold is one key, whatever the modifiers have done since.
    pub fn press(&mut self, code: C, key: Key) -> Key {
        if let Some((_, first)) = self.down.iter().find(|(down, _)| *down == code) {
            return first.clone();
        }
        self.down.push((code, key.clone()));
        key
    }

    /// The key to report for `code` coming up: what its press reported, or `key` — the layout's reading now, which may be no key at all — for a release whose press this never saw.
    pub fn release(&mut self, code: &C, key: Option<Key>) -> Option<Key> {
        match self.down.iter().position(|(down, _)| down == code) {
            Some(at) => Some(self.down.swap_remove(at).1),
            None => key,
        }
    }

    /// Forgets every key down, for a surface that lost the keyboard: the releases for them will go to whoever has it now.
    pub fn clear(&mut self) {
        self.down.clear();
    }
}

#[cfg(test)]
#[path = "key_pairing_test.rs"]
mod tests;
