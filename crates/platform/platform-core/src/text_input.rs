//! Input methods: what a platform tells the field that holds the keyboard about a composition, and what that field tells the platform about where it is.

use std::ops::Range;

use geometry_core::Rect;

/// Where the field that holds the keyboard takes text, for the input method composing into it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextInput {
    /// Which field it is, opaque and stable while the field holds the keyboard: a composition belongs to the field it began in, and a change here is the keyboard moving to another field rather than a caret moving.
    pub field: u64,
    /// The caret, in the surface's logical coordinates: where an input method opens its candidate window.
    pub caret: Rect,
    /// Whether what is typed is hidden, as a password is, so the input method neither shows nor learns it.
    pub secret: bool,
}

/// What an input method did to the field that holds the keyboard.
///
/// A composition is text the input method has not decided yet: the field shows it at the caret, apart from its own text, until a [`Commit`](Self::Commit) makes it real or an empty [`Preedit`](Self::Preedit) takes it away.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ime {
    /// The input method started listening to this surface.
    Enabled,
    /// The composition as it stands, replacing the last one; empty when it was emptied or abandoned.
    ///
    /// `cursor` is the input method's own cursor inside `text`, in bytes and on char boundaries: an empty range is a caret, a non-empty one the part being converted, and `None` asks for no cursor at all.
    Preedit {
        text: String,
        cursor: Option<Range<usize>>,
    },
    /// Text the input method decided on, to insert at the caret in place of the composition.
    Commit(String),
    /// The input method stopped listening, taking any composition with it.
    Disabled,
}

impl Ime {
    /// A composition of `text` with the cursor at `cursor`, held inside `text` and on its char boundaries whatever the platform reported.
    pub fn preedit(text: impl Into<String>, cursor: Option<(usize, usize)>) -> Self {
        let text = text.into();
        let cursor = cursor.map(|(from, to)| {
            let (from, to) = (from.min(to), from.max(to));
            floor_boundary(&text, from)..floor_boundary(&text, to)
        });
        Self::Preedit { text, cursor }
    }

    /// [`preedit`](Self::preedit) with the cursor counted in UTF-16 code units, as a browser counts a selection.
    pub fn preedit_utf16(text: impl Into<String>, cursor: Option<(u32, u32)>) -> Self {
        let text = text.into();
        let cursor =
            cursor.map(|(from, to)| (utf16_to_byte(&text, from), utf16_to_byte(&text, to)));
        Self::preedit(text, cursor)
    }
}

fn floor_boundary(text: &str, at: usize) -> usize {
    let mut at = at.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

fn utf16_to_byte(text: &str, units: u32) -> usize {
    let mut counted = 0u32;
    for (at, character) in text.char_indices() {
        if counted >= units {
            return at;
        }
        counted += character.len_utf16() as u32;
    }
    text.len()
}

#[cfg(test)]
#[path = "text_input_test.rs"]
mod tests;
