//! The arithmetic an editor does on its text and nothing else: char boundaries, the word or line a press lands in, and how many presses in a row a press is.

use std::ops::Range;
use std::time::Duration;

use web_time::Instant;

/// The longest gap between two presses that still reads as one more click of the same gesture: a double-click, then a triple-click.
pub const DOUBLE_CLICK: Duration = Duration::from_millis(400);

/// The char boundary at or before byte `at`, within `text`.
pub(crate) fn floor_boundary(text: &str, at: usize) -> usize {
    let mut at = at.min(text.len());
    while at > 0 && !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// The char boundary strictly before byte `at`, or 0.
pub(crate) fn prev_boundary(text: &str, at: usize) -> usize {
    let at = floor_boundary(text, at);
    text[..at].char_indices().next_back().map_or(0, |(i, _)| i)
}

/// The char boundary strictly after byte `at`, or the end.
pub(crate) fn next_boundary(text: &str, at: usize) -> usize {
    let at = floor_boundary(text, at);
    text[at..]
        .chars()
        .next()
        .map_or(text.len(), |c| at + c.len_utf8())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Class {
    Word,
    Space,
    Break,
    Mark,
}

fn class(c: char) -> Class {
    match c {
        '\n' => Class::Break,
        c if c.is_whitespace() => Class::Space,
        c if c.is_alphanumeric() || c == '_' => Class::Word,
        _ => Class::Mark,
    }
}

/// The run of like characters the character starting at byte `under` belongs to: a word, a stretch of spaces, a run of punctuation. What a double-click selects. A press past the end of a line takes the run before it.
pub(crate) fn word_at(text: &str, under: usize) -> Range<usize> {
    let mut at = floor_boundary(text, under);
    let starts_a_break = text[at..].starts_with('\n') || at == text.len();
    if starts_a_break {
        match text[..at].chars().next_back() {
            Some(before) if before != '\n' => at -= before.len_utf8(),
            _ => return at..at,
        }
    }
    let kind = text[at..].chars().next().map_or(Class::Break, class);
    let start = text[..at]
        .char_indices()
        .rev()
        .take_while(|(_, c)| class(*c) == kind)
        .last()
        .map_or(at, |(i, _)| i);
    let end = text[at..]
        .char_indices()
        .find(|(_, c)| class(*c) != kind)
        .map_or(text.len(), |(i, _)| at + i);
    start..end
}

/// The line between the `\n`s around byte `under`, without them: what a triple-click selects in an editor of several lines.
pub(crate) fn line_at(text: &str, under: usize) -> Range<usize> {
    let at = floor_boundary(text, under);
    let start = text[..at].rfind('\n').map_or(0, |i| i + 1);
    let end = text[at..].find('\n').map_or(text.len(), |i| at + i);
    start..end
}

/// How a press extends what it selected as the pointer is dragged on: by letters, by words or by lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Granularity {
    Char,
    Word,
    Line,
}

impl Granularity {
    /// The press that is the `count`th in a row: one positions, two take a word, three a line.
    pub(crate) fn of_clicks(count: u8) -> Self {
        match count {
            0 | 1 => Self::Char,
            2 => Self::Word,
            _ => Self::Line,
        }
    }
}

/// Counts presses that land close together in place and in time, so the second is a double-click and the third a triple-click.
#[derive(Default)]
pub(crate) struct Clicks {
    last: Option<(Instant, f32, f32)>,
    count: u8,
}

impl Clicks {
    /// Records a press at `(x, y)` and answers which click in a row it is, from 1 to 3; a fourth starts over.
    pub(crate) fn press(&mut self, x: f32, y: f32) -> u8 {
        let now = Instant::now();
        let slop = platform_core::TAP_SLOP;
        let follows = self.last.is_some_and(|(then, px, py)| {
            now.duration_since(then) <= DOUBLE_CLICK && (x - px).hypot(y - py) <= slop
        });
        self.count = match follows {
            true if self.count < 3 => self.count + 1,
            _ => 1,
        };
        self.last = Some((now, x, y));
        self.count
    }
}

#[cfg(test)]
#[path = "text_edit_test.rs"]
mod tests;
