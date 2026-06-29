//! Line-oriented lexer for `.rsx` source.
//!
//! `.rsx` is whitespace-sensitive, so the lexer keeps working at the line level
//! instead of producing a flat token stream. It splits the source into three
//! kinds of lines depending on the active section:
//!
//! - Logic lines are captured verbatim (Rust source).
//! - Style and View lines carry their original text plus the leading indentation
//!   width, which the parser uses to reconstruct the view hierarchy.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Unknown,
    Logic,
    Style,
    View,
    /// A `[preview "Name" …]` header line. Its body is lexed as [`Section::View`] (the preview body
    /// is view markup), so the parser reuses the whole view machinery for it.
    Preview,
}

#[derive(Debug, Clone)]
pub struct Line {
    pub section: Section,
    /// 1-based line number in the original source.
    pub number: usize,
    /// Number of leading spaces (tabs count as one space each).
    pub indent: usize,
    /// The line content with leading indentation stripped (trailing whitespace trimmed too).
    pub content: String,
    /// Absolute byte offset in the original source where `content` begins (line byte start +
    /// leading-whitespace bytes). Lets the parser turn intra-line char positions into source byte
    /// offsets so the transpiler can map `[view]` Rust expressions back to the `.rsx` precisely.
    pub content_start: usize,
    /// The raw, untouched line (used to preserve the logic zone exactly).
    pub raw: String,
}

impl Line {
    pub fn is_blank(&self) -> bool {
        self.content.is_empty()
    }
}

/// Returns the [`Section`] a `[...]` header line switches into, or `None` for a
/// non-header line. `trimmed` must already have surrounding whitespace removed.
pub fn header_section(trimmed: &str) -> Option<Section> {
    match trimmed {
        "[logic]" => Some(Section::Logic),
        "[style]" => Some(Section::Style),
        "[view]" => Some(Section::View),
        _ => None,
    }
}

/// Splits `source` into classified lines, switching sections on `[logic]` / `[style]` / `[view]` headers.
pub fn lex(source: &str) -> Vec<Line> {
    let mut lines = Vec::new();
    let mut section = Section::Unknown;
    // Running byte offset of the current chunk's start within `source`.
    let mut byte_offset = 0usize;

    for (idx, chunk) in source.split_inclusive('\n').enumerate() {
        let line_byte_start = byte_offset;
        byte_offset += chunk.len();

        let number = idx + 1;
        // Mirror `str::lines()`: drop the trailing `\n` and any `\r` so `raw` stays unchanged.
        let raw = chunk.strip_suffix('\n').unwrap_or(chunk);
        let raw = raw.strip_suffix('\r').unwrap_or(raw);
        let trimmed = raw.trim();

        if let Some(new_section) = header_section(trimmed) {
            section = new_section;
            continue;
        }

        let indent = leading_indent(raw);
        let content_start = line_byte_start + (raw.len() - raw.trim_start().len());
        let content = trimmed.to_string();

        // `[preview "Name" …]` is a repeatable, parameterized header that `header_section`'s exact
        // table can't match: emit it as a `Section::Preview` line (so the parser reads its
        // name/options) and lex the lines that follow as `Section::View` markup.
        let line_section = if is_preview_header(trimmed) {
            Section::Preview
        } else {
            section
        };

        lines.push(Line {
            section: line_section,
            number,
            indent,
            content,
            content_start,
            raw: raw.to_string(),
        });

        if line_section == Section::Preview {
            section = Section::View;
        }
    }

    lines
}

/// Whether `trimmed` is a `[preview …]` header. Parameterized (carries a name/options), so it is
/// matched here rather than in [`header_section`]'s exact table.
fn is_preview_header(trimmed: &str) -> bool {
    let Some(inner) = trimmed.strip_prefix('[').and_then(|s| s.strip_suffix(']')) else {
        return false;
    };
    let inner = inner.trim_start();
    // `preview` must be a whole word, so `[previewish]` is not a preview header.
    inner == "preview"
        || inner
            .strip_prefix("preview")
            .is_some_and(|rest| rest.starts_with(char::is_whitespace))
}

/// Counts leading whitespace columns; a tab is treated as a single column.
fn leading_indent(line: &str) -> usize {
    let mut count = 0;
    for ch in line.chars() {
        match ch {
            ' ' | '\t' => count += 1,
            _ => break,
        }
    }
    count
}
