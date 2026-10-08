//! Recursive-descent parser for `.rsx` documents.
//!
//! Split by document section: [`style`] parses `[style]` classes, [`view`] parses `[view]` element trees, [`preview`] parses the trailing `[previews …]` meta and `[preview ...]` blocks with their `[play]` zones. All three `impl Parser` blocks live in this file's descendant modules and can reach `Parser`'s private fields because Rust privacy is scoped to a module and its descendants.

mod preview;
mod style;
mod view;

use crate::ast::*;
use crate::error::ParseError;
use crate::lexer::{self, Line, Section};

/// Drives parsing of a single `.rsx` source string.
pub struct Parser {
    lines: Vec<Line>,
    pos: usize,
}

impl Parser {
    pub fn new(source: &str) -> Self {
        Self {
            lines: lexer::lex(source),
            pos: 0,
        }
    }

    pub fn parse(mut self) -> Result<RsxDocument, ParseError> {
        if let Some(line) = self
            .lines
            .iter()
            .find(|l| l.section == Section::Unknown && !l.is_blank())
        {
            return Err(ParseError {
                line: line.number,
                message: "content before [logic]: add a [logic] section header".into(),
            });
        }
        let logic = self.parse_logic();
        let style = self.parse_style()?;
        let view = self.parse_view()?;
        let previews_meta = self.parse_previews_meta()?;
        let previews = self.parse_previews()?;
        self.reject_misplaced_preview_sections()?;
        Ok(RsxDocument {
            logic,
            style,
            view,
            previews_meta,
            previews,
        })
    }

    /// Captures consecutive logic lines verbatim (blanks and comments preserved).
    fn parse_logic(&mut self) -> LogicZone {
        let (source, start_line) = self.take_verbatim(|line| line.section == Section::Logic);
        LogicZone { source, start_line }
    }

    /// Consumes the lines `belongs` accepts and joins their raw text, trimming leading and trailing blank lines but keeping interior formatting intact. Returns the text and the 1-based line of its first content line (0 when there is none).
    fn take_verbatim(&mut self, belongs: impl Fn(&Line) -> bool) -> (String, usize) {
        let mut raws = Vec::new();
        let mut numbers = Vec::new();
        while let Some(line) = self.lines.get(self.pos) {
            if !belongs(line) {
                break;
            }
            raws.push(line.raw.as_str());
            numbers.push(line.number);
            self.pos += 1;
        }

        let start = raws.iter().position(|l| !l.trim().is_empty()).unwrap_or(0);
        let end = raws
            .iter()
            .rposition(|l| !l.trim().is_empty())
            .map(|i| i + 1)
            .unwrap_or(0);
        if start < end {
            (raws[start..end].join("\n"), numbers[start])
        } else {
            (String::new(), 0)
        }
    }
}

/// Splits a string on its first `:` that is not part of a closure/`::` path. Shared by style, view and preview header parsing, so it lives here rather than in any one submodule.
fn split_once_colon(s: &str) -> Option<(&str, &str)> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b':' {
            // Skip Rust path separators like `Color::Red`.
            if bytes.get(i + 1) == Some(&b':') {
                i += 2;
                continue;
            }
            return Some((&s[..i], &s[i + 1..]));
        }
        i += 1;
    }
    None
}
