//! [`Span`]: where in the source a piece of syntax, or a complaint about it, sits.

use std::ops::Range;

/// A byte range into the source an expression was compiled from, end exclusive.
///
/// Bytes rather than characters because that is what slicing a `&str` takes; [`crate::render`] turns one into columns for display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// The empty span at `at`: where something was expected and nothing was written.
    pub const fn point(at: usize) -> Self {
        Self::new(at, at)
    }

    /// The smallest span covering both.
    pub fn to(self, other: Span) -> Self {
        Self::new(self.start.min(other.start), self.end.max(other.end))
    }

    pub const fn len(self) -> usize {
        self.end.saturating_sub(self.start)
    }

    pub const fn is_empty(self) -> bool {
        self.len() == 0
    }

    pub const fn range(self) -> Range<usize> {
        self.start..self.end
    }
}

impl From<Span> for Range<usize> {
    fn from(span: Span) -> Self {
        span.range()
    }
}
