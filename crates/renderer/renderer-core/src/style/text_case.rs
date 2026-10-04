//! [`TextCase`]: the case text is shown in whatever case it is written in, and [`case_text`], the one mapping every measurer and shaper applies so they all measure the same string.

use std::borrow::Cow;
use std::ops::Range;

use icu_casemap::options::{TitlecaseOptions, TrailingCase};
use icu_casemap::{CaseMapper, TitlecaseMapper};
use icu_locale_core::LanguageIdentifier;
use unicode_segmentation::UnicodeSegmentation;

use super::{Span, TextStyle};

/// The case a run of text is shown in, as CSS `text-transform` does: the string keeps the case it was written in, which is what a reader reads and what a link or a search finds, and only the glyphs change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TextCase {
    #[default]
    AsWritten,
    Upper,
    Lower,
    /// The first letter of every word in title case, the rest as written.
    Capitalize,
}

impl TextCase {
    /// `text` in this case, by the rules of `lang` (a BCP 47 tag; Turkish dots its capital `İ`, Greek drops its accents), or the language-neutral rules when there is none or it does not parse.
    pub fn apply<'a>(self, text: &'a str, lang: Option<&str>) -> Cow<'a, str> {
        if self == TextCase::AsWritten {
            return Cow::Borrowed(text);
        }
        let lang = lang
            .and_then(|tag| LanguageIdentifier::try_from_str(tag).ok())
            .unwrap_or(LanguageIdentifier::UNKNOWN);
        match self {
            TextCase::AsWritten => Cow::Borrowed(text),
            TextCase::Upper => CaseMapper::new().uppercase_to_string(text, &lang),
            TextCase::Lower => CaseMapper::new().lowercase_to_string(text, &lang),
            TextCase::Capitalize => capitalize(text, &lang),
        }
    }

    pub fn css_name(self) -> &'static str {
        match self {
            TextCase::AsWritten => "none",
            TextCase::Upper => "uppercase",
            TextCase::Lower => "lowercase",
            TextCase::Capitalize => "capitalize",
        }
    }
}

fn capitalize<'a>(text: &'a str, lang: &LanguageIdentifier) -> Cow<'a, str> {
    let mut options = TitlecaseOptions::default();
    options.trailing_case = Some(TrailingCase::Unchanged);
    let mapper = TitlecaseMapper::new();
    let mut out = String::with_capacity(text.len());
    let mut changed = false;
    for word in text.split_word_bounds() {
        let titled = mapper.titlecase_segment_to_string(word, lang, options);
        changed |= matches!(titled, Cow::Owned(_));
        out.push_str(&titled);
    }
    if changed {
        Cow::Owned(out)
    } else {
        Cow::Borrowed(text)
    }
}

/// A paragraph as it is shaped: its text in the case its style shows it in, its spans moved onto that text, and the way back to the bytes it was written in.
#[derive(Debug, Clone, PartialEq)]
pub struct CasedText<'a> {
    pub text: Cow<'a, str>,
    pub spans: Option<Cow<'a, [Span]>>,
    /// Each stretch of the shown text and the stretch of the written one it came from, in order; empty when the two are the same bytes.
    pieces: Vec<(Range<usize>, Range<usize>)>,
}

impl CasedText<'_> {
    /// The byte of the written text that shown byte `index` came from: the start of the stretch it lies in, or the byte itself when nothing was recased.
    pub fn source_index(&self, index: usize) -> usize {
        if self.pieces.is_empty() {
            return index;
        }
        self.pieces
            .iter()
            .find(|(shown, _)| shown.contains(&index))
            .map_or_else(
                || self.pieces.last().map_or(index, |(_, written)| written.end),
                |(shown, written)| {
                    if shown.len() == written.len() {
                        written.start + (index - shown.start)
                    } else {
                        written.start
                    }
                },
            )
    }
}

/// `text` and `spans` as `style` and each span's own declaration show them (see [`TextCase`]), cased run by run so a span's range still covers exactly its own words. Borrows both untouched when nothing in the paragraph asks for a case.
pub fn case_text<'a>(text: &'a str, spans: Option<&'a [Span]>, style: &TextStyle) -> CasedText<'a> {
    let spans = spans.filter(|spans| !spans.is_empty());
    let spans_case =
        spans.is_some_and(|spans| spans.iter().any(|span| span.over.text_case.is_some()));
    if style.text_case == TextCase::AsWritten && !spans_case {
        return CasedText {
            text: Cow::Borrowed(text),
            spans: spans.map(Cow::Borrowed),
            pieces: Vec::new(),
        };
    }
    let lang = style.lang.as_deref();
    let Some(spans) = spans else {
        let cased = style.text_case.apply(text, lang);
        let pieces = match &cased {
            Cow::Borrowed(_) => Vec::new(),
            Cow::Owned(shown) => vec![(0..shown.len(), 0..text.len())],
        };
        return CasedText {
            text: cased,
            spans: None,
            pieces,
        };
    };

    let mut out = String::with_capacity(text.len());
    let mut pieces = Vec::with_capacity(spans.len() * 2 + 1);
    let mut moved = Vec::with_capacity(spans.len());
    let mut push = |written: Range<usize>, case: TextCase, out: &mut String| {
        let start = out.len();
        out.push_str(&case.apply(&text[written.clone()], lang));
        pieces.push((start..out.len(), written));
        start..out.len()
    };
    let mut at = 0usize;
    for span in spans {
        let start = (span.range.start as usize).clamp(at, text.len());
        let end = (span.range.end as usize).clamp(start, text.len());
        if !text.is_char_boundary(start) || !text.is_char_boundary(end) {
            continue;
        }
        if start > at {
            push(at..start, style.text_case, &mut out);
        }
        let case = span.over.text_case.unwrap_or(style.text_case);
        let shown = push(start..end, case, &mut out);
        moved.push(Span {
            range: shown.start as u32..shown.end as u32,
            ..span.clone()
        });
        at = end;
    }
    if at < text.len() {
        push(at..text.len(), style.text_case, &mut out);
    }
    if out == text {
        return CasedText {
            text: Cow::Borrowed(text),
            spans: Some(Cow::Borrowed(spans)),
            pieces: Vec::new(),
        };
    }
    CasedText {
        text: Cow::Owned(out),
        spans: Some(Cow::Owned(moved)),
        pieces,
    }
}

#[cfg(test)]
#[path = "text_case_test.rs"]
mod tests;
