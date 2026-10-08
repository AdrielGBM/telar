//! The trailing preview sections: an optional `[previews …]` meta section with its prose, then `[preview "Name" …]` variants, each optionally followed by a `[play]` zone. A variant's body is the following `Section::View` markup, parsed with the same view machinery as `[view]`.

use super::view::{byte_at, read_balanced_parens, read_colon_value, read_quoted, value_end};
use super::{Parser, split_once_colon};
use crate::ast::*;
use crate::error::ParseError;
use crate::lexer::{HeaderRest, Line, Section, strip_preview_header, strip_previews_header};

impl Parser {
    pub(super) fn parse_previews_meta(&mut self) -> Result<Option<PreviewsMeta>, ParseError> {
        self.skip_blank_view_lines();
        let Some(line) = self
            .lines
            .get(self.pos)
            .filter(|l| l.section == Section::Previews)
        else {
            return Ok(None);
        };
        let header_line = line.number;
        let header = read_header(line, strip_previews_header)?;
        if header.args.is_some() {
            return Err(ParseError {
                message: "`args(…)` declares one variant's signals, so it belongs on a `[preview]` header".into(),
                line: header_line,
            });
        }
        self.pos += 1;
        let (body, _) = self.take_verbatim(|l| l.section == Section::Prose);
        Ok(Some(PreviewsMeta {
            title: header.name,
            options: header.options,
            body,
            line: header_line,
        }))
    }

    pub(super) fn parse_previews(&mut self) -> Result<Vec<Preview>, ParseError> {
        let mut previews = Vec::new();
        loop {
            self.skip_blank_view_lines();
            let Some(line) = self
                .lines
                .get(self.pos)
                .filter(|l| l.section == Section::Preview)
            else {
                break;
            };
            let header_line = line.number;
            let header = read_header(line, strip_preview_header)?;
            let name = header.name.ok_or_else(|| ParseError {
                message: "preview needs a quoted name, e.g. `[preview \"My preview\"]`".into(),
                line: header_line,
            })?;
            self.pos += 1;
            let body = self.parse_preview_body()?;
            let play = self.parse_play();
            previews.push(Preview {
                name,
                options: header.options,
                args: header.args.unwrap_or_default(),
                body,
                play,
                line: header_line,
            });
        }
        Ok(previews)
    }

    /// Collects one preview's body: the `Section::View` markup at its own base indentation, stopping at the next header or EOF.
    fn parse_preview_body(&mut self) -> Result<Vec<ViewNode>, ParseError> {
        self.skip_blank_view_lines();
        let base = match self.lines.get(self.pos) {
            Some(l) if l.section == Section::View && !l.is_blank() => l.indent,
            _ => return Ok(Vec::new()),
        };
        self.parse_view_nodes(base)
    }

    /// The `[play]` zone right after a preview's body, if one is written there.
    fn parse_play(&mut self) -> Option<PlayZone> {
        self.skip_blank_view_lines();
        let line = self
            .lines
            .get(self.pos)
            .filter(|l| l.is_play_header())?
            .number;
        self.pos += 1;
        let (source, start_line) =
            self.take_verbatim(|l| l.section == Section::Play && !l.is_play_header());
        Some(PlayZone {
            source,
            line,
            start_line,
        })
    }

    /// Refuses a `[previews]` or `[play]` the preview sections did not consume, which would otherwise vanish without a word.
    pub(super) fn reject_misplaced_preview_sections(&self) -> Result<(), ParseError> {
        let Some(line) = self.lines[self.pos..]
            .iter()
            .find(|l| matches!(l.section, Section::Previews | Section::Play))
        else {
            return Ok(());
        };
        let message = match line.section {
            Section::Previews => {
                "`[previews]` is the meta section of the whole file: write it once, before the first `[preview]`"
            }
            _ => "`[play]` runs against the `[preview]` right above it, and each preview takes one",
        };
        Err(ParseError {
            message: message.into(),
            line: line.number,
        })
    }
}

/// What a `[preview …]` or `[previews …]` header holds after its keyword.
struct Header {
    name: Option<String>,
    options: Vec<StyleProp>,
    args: Option<Vec<ArgDecl>>,
}

/// Parses a header's optional quoted name, its options and its `args(…)`. Options are `key:value` (or bare flags with an empty value), kept raw; a value may hold spaces inside brackets or quotes, so `matrix:(mode:[light dark])` is one option.
fn read_header(
    line: &Line,
    strip: fn(&str) -> Option<HeaderRest<'_>>,
) -> Result<Header, ParseError> {
    // The lexer only classifies a header line when its shape matches, so the strip always succeeds here.
    let HeaderRest { offset, text: rest } = strip(&line.content).unwrap_or(HeaderRest {
        offset: line.content.len(),
        text: "",
    });
    let base = line.content_start + offset;
    let error = |message: String| ParseError {
        message,
        line: line.number,
    };

    let chars: Vec<char> = rest.chars().collect();
    let mut name = None;
    let mut from = 0;
    if chars.first() == Some(&'"') {
        let (text, next) =
            read_quoted(&chars, 0).ok_or_else(|| error("unterminated name string".into()))?;
        name = Some(text);
        from = next;
    }

    let mut options = Vec::new();
    let mut args = None;
    for (start, end) in header_tokens(&chars, from) {
        let token: String = chars[start..end].iter().collect();
        if token.starts_with('"') {
            return Err(error(
                "a header's quoted name comes right after its keyword".into(),
            ));
        }
        let Some(paren) = paren_before_colon(&chars[start..end]).map(|p| start + p) else {
            options.push(match split_once_colon(&token) {
                Some((key, value)) => StyleProp {
                    key: key.trim().to_string(),
                    value: value.trim().to_string(),
                },
                None => StyleProp {
                    key: token,
                    value: String::new(),
                },
            });
            continue;
        };
        let key: String = chars[start..paren].iter().collect();
        if key != "args" {
            return Err(error(format!(
                "`{key}(…)` is not a header option: only `args(…)` takes parens, and an option's value takes the colon, `{key}:(…)`"
            )));
        }
        if args.is_some() {
            return Err(error("a preview takes one `args(…)`".into()));
        }
        let (_, next) = read_balanced_parens(&chars[..end], paren)
            .ok_or_else(|| error("unterminated `(` in `args(…)`".into()))?;
        if next != end {
            return Err(error("`args(…)` ends at its closing paren".into()));
        }
        args = Some(read_args(&chars[..next - 1], paren + 1, base, line.number)?);
    }

    Ok(Header {
        name,
        options,
        args,
    })
}

/// Reads the `name:default` pairs of an `args(…)` from `from` to the end of `chars`, which stops short of its closing paren. Each default is read exactly like an attribute value.
fn read_args(
    chars: &[char],
    from: usize,
    base: usize,
    line: usize,
) -> Result<Vec<ArgDecl>, ParseError> {
    let error = |message: String| ParseError { message, line };
    let mut args: Vec<ArgDecl> = Vec::new();
    let mut i = from;
    loop {
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        if i >= chars.len() {
            return Ok(args);
        }
        let name_start = i;
        while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
            i += 1;
        }
        let name: String = chars[name_start..i].iter().collect();
        if name.is_empty()
            || name.starts_with(|c: char| c.is_ascii_digit())
            || chars.get(i) != Some(&':')
        {
            return Err(error(
                "`args(…)` declares `name:default` pairs, e.g. `args(agree:false)`".into(),
            ));
        }
        if args.iter().any(|arg| arg.name == name) {
            return Err(error(format!("`args(…)` declares `{name}` twice")));
        }
        let (default, next, text_at) = read_colon_value(chars, &name, i + 1, line)?;
        if default == Value::Expr(String::new()) {
            return Err(error(format!(
                "`{name}:` needs a default, e.g. `{name}:false`"
            )));
        }
        args.push(ArgDecl {
            name,
            default,
            default_start: base + byte_at(chars, text_at),
        });
        i = next;
    }
}

/// Splits header options at the spaces outside any bracket or string literal, returning each token's char range.
fn header_tokens(chars: &[char], from: usize) -> Vec<(usize, usize)> {
    let mut tokens = Vec::new();
    let mut i = from;
    while i < chars.len() {
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        if i >= chars.len() {
            break;
        }
        let end = value_end(chars, i);
        tokens.push((i, end));
        i = end;
    }
    tokens
}

/// The index of a token's `(` when it opens before any `:`, which makes the token the `key(…)` form rather than an option. A `::` path separator is not a colon.
fn paren_before_colon(token: &[char]) -> Option<usize> {
    let mut i = 0;
    while i < token.len() {
        match token[i] {
            ':' if token.get(i + 1) == Some(&':') => i += 1,
            ':' => return None,
            '(' => return Some(i),
            _ => {}
        }
        i += 1;
    }
    None
}
