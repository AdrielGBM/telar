//! Line-oriented lexer for `.rsx` source.
//!
//! `.rsx` is whitespace-sensitive, so the lexer keeps working at the line level
//! instead of producing a flat token stream. It splits the source into three
//! kinds of lines depending on the active section:
//!
//! - Logic lines are captured verbatim (Rust source).
//! - Style and View lines carry their original text plus the leading indentation
//!   width, which the parser uses to reconstruct the view hierarchy.

/// Which section a line belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Logic,
    Style,
    View,
}

/// A single classified physical line.
#[derive(Debug, Clone)]
pub struct Line {
    pub section: Section,
    /// 1-based line number in the original source.
    pub number: usize,
    /// Number of leading spaces (tabs count as one space each).
    pub indent: usize,
    /// The line content with leading indentation stripped (trailing whitespace trimmed too).
    pub content: String,
    /// The raw, untouched line (used to preserve the logic zone exactly).
    pub raw: String,
}

impl Line {
    /// True when the trimmed content is empty.
    pub fn is_blank(&self) -> bool {
        self.content.is_empty()
    }
}

/// Splits `source` into classified lines, switching sections on `[style]` / `[view]` headers.
pub fn lex(source: &str) -> Vec<Line> {
    let mut lines = Vec::new();
    let mut section = Section::Logic;

    for (idx, raw) in source.lines().enumerate() {
        let number = idx + 1;
        let trimmed = raw.trim();

        if trimmed == "[style]" {
            section = Section::Style;
            continue;
        }
        if trimmed == "[view]" {
            section = Section::View;
            continue;
        }

        let indent = leading_indent(raw);
        let content = raw.trim().to_string();

        lines.push(Line {
            section,
            number,
            indent,
            content,
            raw: raw.to_string(),
        });
    }

    lines
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
