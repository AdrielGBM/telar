//! Recursive-descent parser for `.rsx` documents.
//!
//! Split by document section: [`style`] parses `[style]` constants/classes, [`view`] parses
//! `[view]` element trees, [`preview`] parses trailing `[preview ...]` blocks. All three `impl
//! Parser` blocks live in this file's descendant modules and can reach `Parser`'s private fields
//! because Rust privacy is scoped to a module and its descendants.

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
        let previews = self.parse_previews()?;
        Ok(RsxDocument {
            logic,
            style,
            view,
            previews,
        })
    }

    /// Captures consecutive logic lines verbatim (blanks and comments preserved).
    fn parse_logic(&mut self) -> LogicZone {
        let mut raws = Vec::new();
        let mut numbers = Vec::new();
        while let Some(line) = self.lines.get(self.pos) {
            if line.section != Section::Logic {
                break;
            }
            raws.push(line.raw.clone());
            numbers.push(line.number);
            self.pos += 1;
        }

        // Trim leading/trailing blank lines but keep interior formatting intact.
        let start = raws.iter().position(|l| !l.trim().is_empty()).unwrap_or(0);
        let end = raws
            .iter()
            .rposition(|l| !l.trim().is_empty())
            .map(|i| i + 1)
            .unwrap_or(0);
        let (source, start_line) = if start < end {
            (raws[start..end].join("\n"), numbers[start])
        } else {
            (String::new(), 0)
        };

        LogicZone { source, start_line }
    }
}

/// Splits a string on its first `:` that is not part of a closure/`::` path. Shared by style,
/// view and preview header parsing, so it lives here rather than in any one submodule.
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
