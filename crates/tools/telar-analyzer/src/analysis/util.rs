//! Locating the token under a cursor, and classifying what kind of thing it is.

use crate::position::{Section, find_section_at};
use crate::text::word_at_cursor;
use telar_transpiler::color_attr_keys;

/// The attribute key a value position belongs to, or `None` when the cursor is not in one.
pub fn attribute_key_before_colon(line: &str, word_start: usize) -> Option<&str> {
    let before_colon = line[..word_start.saturating_sub(1)].trim_end();
    let key_start = before_colon
        .rfind(char::is_whitespace)
        .map(|i| i + 1)
        .unwrap_or(0);
    Some(before_colon[key_start..].trim())
}

/// What a `[view]` cursor is sitting on. Goto-definition and hover ask the same question of the same line and diverge only in what they do with the answer, so the question is asked once, here.
pub enum ViewToken<'a> {
    /// `@name`: a style-class reference, without its sigil.
    Class(&'a str),
    /// The value of a colour-typed attribute (`fill:accent`).
    ColorValue(&'a str),
    /// An attribute key, with the tag it is written on — one spelling is still two properties.
    Attr { tag: &'a str, key: &'a str },
    /// The leading token of a line — a builtin tag or a component call, raw, for the caller to tell apart.
    Tag(&'a str),
}

/// Classifies the `[view]` token under the cursor. `None` outside `[view]`, off a word, on the value of an attribute that carries no colour, or anywhere else nothing resolvable is written.
pub fn view_token_at(source: &str, line: u32, character: u32) -> Option<ViewToken<'_>> {
    if find_section_at(source, line) != Section::View {
        return None;
    }
    let line_text = source.lines().nth(line as usize)?;
    let (word_start, word) = word_at_cursor(line_text, character);
    if word.is_empty() {
        return None;
    }
    // `word_at_cursor` keeps the sigil in the word (it breaks on whitespace/`:`/`"`, not `@`), so a class is recognised by the prefix rather than by the char before it.
    if let Some(class) = word.strip_prefix('@') {
        return Some(ViewToken::Class(class));
    }
    if line_text[..word_start].chars().last() == Some(':') {
        let key = attribute_key_before_colon(line_text, word_start)?;
        return color_attr_keys()
            .contains(&key)
            .then_some(ViewToken::ColorValue(word));
    }
    let is_leading = line_text[..word_start].trim().is_empty();
    if !is_leading && line_text[word_start + word.len()..].starts_with(':') {
        let tag = line_text.split_whitespace().next()?;
        return Some(ViewToken::Attr { tag, key: word });
    }
    is_leading.then_some(ViewToken::Tag(word))
}
