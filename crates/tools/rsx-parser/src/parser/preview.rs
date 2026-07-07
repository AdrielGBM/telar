//! Trailing `[preview "Name" …]` sections. Each header is a `Section::Preview` line; its body is
//! the following `Section::View` markup, parsed with the same view machinery as `[view]`.

use super::view::read_quoted;
use super::{Parser, split_once_colon};
use crate::ast::*;
use crate::error::ParseError;
use crate::lexer::{Section, strip_preview_header};

impl Parser {
    pub(super) fn parse_previews(&mut self) -> Result<Vec<Preview>, ParseError> {
        let mut previews = Vec::new();
        loop {
            self.skip_blank_view_lines();
            let Some(line) = self.lines.get(self.pos) else {
                break;
            };
            if line.section != Section::Preview {
                break;
            }
            let header_line = line.number;
            let (name, options) = parse_preview_header(&line.content, header_line)?;
            self.pos += 1;
            let body = self.parse_preview_body()?;
            previews.push(Preview {
                name,
                options,
                body,
                line: header_line,
            });
        }
        Ok(previews)
    }

    /// Collects one preview's body: the `Section::View` markup at its own base indentation, stopping
    /// at the next preview header (a `Section::Preview` line) or EOF.
    fn parse_preview_body(&mut self) -> Result<Vec<ViewNode>, ParseError> {
        self.skip_blank_view_lines();
        let base = match self.lines.get(self.pos) {
            Some(l) if l.section == Section::View && !l.is_blank() => l.indent,
            _ => return Ok(Vec::new()),
        };
        self.parse_view_nodes(base)
    }
}

/// Parses a `[preview "Name" key:value flag …]` header into its name and options. The name is a
/// required quoted string; options are `key:value` pairs (or bare flags with an empty value).
fn parse_preview_header(
    content: &str,
    line: usize,
) -> Result<(String, Vec<StyleProp>), ParseError> {
    // The lexer only classifies a line as `Section::Preview` when `strip_preview_header` matches, so
    // the bracket/`preview`-keyword shape is already guaranteed; only name + options remain to parse.
    let rest = strip_preview_header(content).unwrap_or("");

    let chars: Vec<char> = rest.chars().collect();
    if chars.first() != Some(&'"') {
        return Err(ParseError {
            message: "preview needs a quoted name, e.g. `[preview \"My preview\"]`".to_string(),
            line,
        });
    }
    let (name, next) = read_quoted(&chars, 0).ok_or_else(|| ParseError {
        message: "unterminated preview name string".to_string(),
        line,
    })?;

    let opts: String = chars[next..].iter().collect();
    let mut options = Vec::new();
    for token in opts.split_whitespace() {
        match split_once_colon(token) {
            Some((k, v)) => options.push(StyleProp {
                key: k.trim().to_string(),
                value: v.trim().to_string(),
            }),
            // A bare flag carries an empty value.
            None => options.push(StyleProp {
                key: token.to_string(),
                value: String::new(),
            }),
        }
    }
    Ok((name, options))
}
