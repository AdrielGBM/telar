use std::iter::Enumerate;
use std::ops::Range;
use std::slice;

use super::Span;

/// One stretch of a paragraph: the bytes it covers, and the span that styles it (with its index in the slice given) or `None` for text between spans.
#[derive(Debug, Clone, PartialEq)]
pub struct Segment<'a> {
    pub range: Range<usize>,
    pub span: Option<(usize, &'a Span)>,
}

/// `text` cut at its spans, in order, as consecutive non-empty segments that cover all of it, with the gaps between spans as segments of their own.
///
/// Spans are byte offsets that can outlive the text they indexed, so they are never trusted: a span starting before the end of the previous one begins there, one reaching past the end stops at it, and one that is empty or ends off a character boundary is dropped.
pub fn span_segments<'a>(text: &'a str, spans: &'a [Span]) -> impl Iterator<Item = Segment<'a>> {
    SpanSegments {
        text,
        spans: spans.iter().enumerate(),
        at: 0,
        queued: None,
        spans_done: false,
    }
}

struct SpanSegments<'a> {
    text: &'a str,
    spans: Enumerate<slice::Iter<'a, Span>>,
    at: usize,
    queued: Option<Segment<'a>>,
    spans_done: bool,
}

impl<'a> Iterator for SpanSegments<'a> {
    type Item = Segment<'a>;

    fn next(&mut self) -> Option<Segment<'a>> {
        if let Some(queued) = self.queued.take() {
            return Some(queued);
        }
        let len = self.text.len();
        for (index, span) in self.spans.by_ref() {
            let start = (span.range.start as usize).clamp(self.at, len);
            let end = (span.range.end as usize).clamp(start, len);
            if start == end
                || !self.text.is_char_boundary(start)
                || !self.text.is_char_boundary(end)
            {
                continue;
            }
            let styled = Segment {
                range: start..end,
                span: Some((index, span)),
            };
            let gap = self.at..start;
            self.at = end;
            if gap.is_empty() {
                return Some(styled);
            }
            self.queued = Some(styled);
            return Some(Segment {
                range: gap,
                span: None,
            });
        }
        if self.spans_done || self.at == len {
            return None;
        }
        self.spans_done = true;
        let tail = self.at..len;
        self.at = len;
        Some(Segment {
            range: tail,
            span: None,
        })
    }
}

#[cfg(test)]
#[path = "span_segments.test.rs"]
mod tests;
