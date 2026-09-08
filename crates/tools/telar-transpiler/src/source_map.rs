//! The map from generated Rust back to the `.rsx` that produced it, and the single place that reads it.
//!
//! Two tools need this and they used to have half of it each. The editor got the whole map in memory and could underline the exact expression rustc complained about; the CLI got a sidecar carrying only the line numbers, so `cargo telar check` underlined whole lines however precise rustc had been. Persisting the other half is what closes that, and putting the mapping itself here is what keeps the two from drifting into two answers for the same question.

use serde::{Deserialize, Serialize};
use telar_parser::{Section, find_section_at};

/// A `[view]` Rust expression copied byte-for-byte from the `.rsx` into the generated Rust, so `gen_start + (offset - rsx_start)` maps a generated offset onto the source on a UTF-8 char boundary.
///
/// Only verbatim fragments get one — interpolation `{expr}`, `if`/`let` expressions, verbatim closure and pass-through attribute values. A fragment the transpiler rewrote into something else (a `for` pattern it re-tokenized, a numeric or colour attribute it converted) produces none, and that absence is the signal that its columns mean nothing.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ExprSpan {
    /// Byte offset of the fragment's start in the `.rsx` source.
    pub rsx_start: u32,
    /// Byte length of the fragment (identical in source and generated).
    pub len: u32,
    /// Byte offset of the fragment's start in the generated Rust.
    pub gen_start: u32,
}

/// A binding the transpiler introduced that stands for one the `.rsx` declares: the `let x = x.clone();` the clone pass writes so a `move` closure captures without consuming.
///
/// rust-analyzer sees two bindings and is right to — the shadow's uses are references to *it*, not to the source symbol. Only the transpiler knows they are the same thing, so it says so here rather than leaving the analyzer to infer it from the text. A rename that skipped this would edit the declaration and leave every use inside the closure behind.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShadowBinding {
    /// Byte offset, in the generated Rust, of the name the shadow declares.
    pub gen_decl: u32,
    /// The `.rsx` binding it stands for. A name is enough to identify it: the clone pass shadows only captures that are not loop variables, so within one generated file the name resolves to the `[logic]` binding or prop of that name and nothing else.
    pub name: String,
}

/// Everything the generated Rust knows about where it came from.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct SourceMap {
    /// Per generated line (0-based), the 0-based `.rsx` line it originated from, or `None` for boilerplate and transpiler-injected lines.
    pub lines: Vec<Option<u32>>,
    /// The verbatim fragments, which are what make a *column* trustworthy rather than just a line.
    pub exprs: Vec<ExprSpan>,
    /// The bindings the transpiler introduced that stand for source ones, which a rename has to reach through.
    #[serde(default)]
    pub shadows: Vec<ShadowBinding>,
}

/// Where a span of generated Rust belongs in the `.rsx` that produced it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RsxSpan {
    /// A byte range in the `.rsx`, trustworthy down to the column.
    Exact { start: u32, end: u32 },
    /// A 0-based `.rsx` line and nothing narrower. The transpiler rewrote this line into something with no column correspondence, and a narrow range would underline text that has nothing to do with the error — which is worse than underlining the line.
    Line(u32),
}

impl SourceMap {
    /// Without shadows: the `.rs.map` on disk is read by editors that only ask about positions, and a rename resolves against the in-memory map instead.
    pub fn new(lines: Vec<Option<u32>>, exprs: Vec<ExprSpan>) -> Self {
        Self {
            lines,
            exprs,
            shadows: Vec::new(),
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }

    pub fn from_json(text: &str) -> Option<Self> {
        serde_json::from_str(text).ok()
    }

    /// Maps a byte range in the generated Rust back onto the `.rsx`.
    ///
    /// Three outcomes, and the difference between the last two is the whole point. A verbatim `[view]` fragment maps byte for byte through its [`ExprSpan`]. A `[logic]` line maps by line plus the indentation the transpiler added, which holds because `[logic]` is transpiled 1:1. Anything else in `[view]` widens to its line. A generated line with no origin at all — boilerplate — maps to nothing.
    pub fn locate(
        &self,
        generated: &str,
        gen_start: u32,
        gen_end: u32,
        rsx: &str,
    ) -> Option<RsxSpan> {
        if let Some(span) = self
            .exprs
            .iter()
            .find(|s| gen_start >= s.gen_start && gen_start < s.gen_start + s.len)
        {
            let span_end = span.gen_start + span.len;
            return Some(RsxSpan::Exact {
                start: span.rsx_start + (gen_start - span.gen_start),
                end: span.rsx_start + (gen_end.clamp(gen_start, span_end) - span.gen_start),
            });
        }

        let (gen_line, gen_line_start) = line_at(generated, gen_start)?;
        let rsx_line = (*self.lines.get(gen_line)?)?;
        if find_section_at(rsx, rsx_line) != Section::Logic {
            return Some(RsxSpan::Line(rsx_line));
        }

        let Some(rsx_line_start) = line_start(rsx, rsx_line as usize) else {
            return Some(RsxSpan::Line(rsx_line));
        };
        // Leading whitespace is spaces and tabs, so its byte width is also its column count.
        let delta = leading_ws(&generated[gen_line_start..])
            .saturating_sub(leading_ws(&rsx[rsx_line_start..]));
        let rsx_line_end = rsx_line_start + nth_line(rsx, rsx_line as usize).map_or(0, str::len);
        let shift = |offset: u32| {
            let column = (offset as usize).saturating_sub(gen_line_start);
            (rsx_line_start + column.saturating_sub(delta)).min(rsx_line_end) as u32
        };
        Some(RsxSpan::Exact {
            start: shift(gen_start),
            end: shift(gen_end.max(gen_start)),
        })
    }
}

/// The 0-based line containing `offset`, and that line's own byte start. `None` past the end of `text`.
fn line_at(text: &str, offset: u32) -> Option<(usize, usize)> {
    let offset = offset as usize;
    if offset > text.len() {
        return None;
    }
    let start = text[..offset].rfind('\n').map_or(0, |at| at + 1);
    Some((text[..start].matches('\n').count(), start))
}

/// Byte offset where the 0-based line `n` begins.
fn line_start(text: &str, n: usize) -> Option<usize> {
    let mut offset = 0;
    for (i, chunk) in text.split_inclusive('\n').enumerate() {
        if i == n {
            return Some(offset);
        }
        offset += chunk.len();
    }
    (n == 0).then_some(0)
}

/// The 0-based `n`-th line of `text`, without its trailing newline.
pub fn nth_line(text: &str, n: usize) -> Option<&str> {
    text.split_inclusive('\n')
        .nth(n)
        .map(|line| line.strip_suffix('\n').unwrap_or(line))
}

/// Byte width of the leading space/tab run at the start of `text`.
fn leading_ws(text: &str) -> usize {
    text.bytes()
        .take_while(|b| *b == b' ' || *b == b'\t')
        .count()
}

#[cfg(test)]
#[path = "source_map_test.rs"]
mod tests;
