//! A `[preview "Name" …]` or `[previews "Group/Title" …]` header line read token by token: its option keys, its `args(…)` names and the axes of an inline `matrix:(…)`, for highlighting and hover, and where a cursor sits among them, for completion.
//!
//! The parser keeps options raw and refuses a malformed header outright, so it cannot answer for a header still being typed; this reads the same grammar leniently, from the text alone.

use telar_transpiler::PREVIEW_OPTION_KEYS;

/// Which preview header a line opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderKind {
    /// `[preview "Name" …]`, a variant.
    Variant,
    /// `[previews "Group/Title" …]`, the file's meta section.
    Meta,
}

/// One option a preview header takes, as the editor describes it.
#[derive(Debug, Clone, Copy)]
pub struct PreviewOption {
    pub key: &'static str,
    pub doc: &'static str,
    /// What completing the key inserts, as a snippet.
    pub snippet: &'static str,
}

/// The docs and completion snippet of the option `key`, or `None` for a key the editor does not describe.
fn describe(key: &str) -> Option<(&'static str, &'static str)> {
    Some(match key {
        "layout" => (
            "How the canvas places the preview: `padded`, `centered` or `fullscreen`.",
            "layout:",
        ),
        "viewport" => (
            "The canvas size in logical px, `WIDTHxHEIGHT`.",
            "viewport:${1:390}x${2:844}",
        ),
        "bg" => ("The canvas background, a hex colour.", "bg:#${1:202020}"),
        "mode" => (
            "The theme mode the canvas renders in, by the id it is registered under.",
            "mode:${1:dark}",
        ),
        "locale" => (
            "The locale the canvas renders in, a BCP 47 language tag.",
            "locale:${1:en}",
        ),
        "dir" => (
            "The canvas's writing direction, `ltr` or `rtl`, whatever the locale's own.",
            "dir:",
        ),
        "tags" => (
            "Free-form labels the workshop filters by: `tags:[stateful slow]`.",
            "tags:[${1:name}]",
        ),
        "matrix" => (
            "Renders the preview once per cell: a matrix named in `[telar.previews.matrices]` or the built-in `themes`, or its axes written out, `matrix:(mode:[light dark] size:[12 16])`.",
            "matrix:",
        ),
        "decorator" => (
            "A fn that wraps the preview's tree, by its path: `decorator:frames::card`.",
            "decorator:${1:path}",
        ),
        "fixture" => (
            "A fn called before the preview builds, by its path: `fixture:seed_state`.",
            "fixture:${1:path}",
        ),
        "surface" => (
            "Mounts the preview as a surface of this size, `WIDTHxHEIGHT`, the way a compositor would.",
            "surface:${1:360}x${2:240}",
        ),
        "animate" => (
            "A flag beside `surface:`: plays the root's enter transition.",
            "animate",
        ),
        "args" => (
            "`args:none` keeps the root's literal attributes fixed rather than turning each into an arg.",
            "args:",
        ),
        _ => return None,
    })
}

/// The option `key` names, if a preview header takes one by that name.
pub fn option(key: &str) -> Option<PreviewOption> {
    let key = PREVIEW_OPTION_KEYS.iter().copied().find(|k| *k == key)?;
    let (doc, snippet) = describe(key)?;
    Some(PreviewOption { key, doc, snippet })
}

/// Every option a preview header takes, in the order the transpiler lists them. `args(…)` is the one form that is not `key:value`, and only a variant takes it.
pub fn options() -> impl Iterator<Item = PreviewOption> {
    PREVIEW_OPTION_KEYS.iter().filter_map(|key| option(key))
}

/// What `args(…)` does, shown where it is completed and hovered.
pub const ARGS_DECL_DOC: &str = "Declares live, preview-scoped signals, read in the body as `$name`: `args(agree:false)`. Each gets a control in the workshop.";

/// A name in a header that means something on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// The key of an option, `args` of `args(…)` included.
    OptionKey,
    /// A name `args(…)` declares.
    ArgName,
    /// An axis of an inline `matrix:(…)`.
    MatrixAxis,
}

/// A [`TokenKind`] and the bytes of the line it spans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeaderToken {
    pub kind: TokenKind,
    pub start: usize,
    pub len: usize,
}

/// Where the end of a header's text falls, which is what completion asks of the text before the cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    /// Where an option's key is, or is about to be, written.
    OptionKey,
    /// In the value of the option `key`.
    OptionValue(String),
    /// Where an `args(…)` name is written.
    ArgName,
    /// In the default of an `args(…)` name.
    ArgValue,
    /// Where an inline matrix's axis is written.
    MatrixAxis,
    /// In the values of the inline matrix axis `axis`.
    MatrixAxisValue(String),
    /// Anywhere else: inside a quoted string, before the name, past the closing `]`.
    Elsewhere,
}

/// Which header `line` opens, and the byte offset just past its keyword. Lenient about the closing `]`, which a header being typed may not have yet.
pub fn header_kind(line: &str) -> Option<(HeaderKind, usize)> {
    let lead = line.len() - line.trim_start().len();
    let after_bracket = line[lead..].strip_prefix('[')?;
    let keyword_at = lead + 1 + (after_bracket.len() - after_bracket.trim_start().len());
    let rest = &line[keyword_at..];
    let whole_word = |keyword: &str| {
        rest.strip_prefix(keyword)
            .filter(|after| {
                after.is_empty() || after.starts_with(|c: char| c.is_whitespace() || c == ']')
            })
            .map(|_| keyword_at + keyword.len())
    };
    whole_word("previews")
        .map(|end| (HeaderKind::Meta, end))
        .or_else(|| whole_word("preview").map(|end| (HeaderKind::Variant, end)))
}

/// The named tokens of a header line, in order. Empty for a line that is not a preview header.
pub fn header_tokens(line: &str) -> Vec<HeaderToken> {
    match header_kind(line) {
        Some((_, from)) => {
            let mut scanner = Scanner::new(line, from);
            let _ = scanner.header();
            scanner.tokens
        }
        None => Vec::new(),
    }
}

/// Where the end of `prefix`, a header line cut at the cursor, falls. `None` when it is not a preview header.
pub fn place_at_end(prefix: &str) -> Option<(HeaderKind, Place)> {
    let (kind, from) = header_kind(prefix)?;
    let mut scanner = Scanner::new(prefix, from);
    let place = match scanner.header() {
        Ok(()) => Place::Elsewhere,
        Err(place) => place,
    };
    Some((kind, place))
}

/// The token under the UTF-8 `cursor`, the end of a name included.
pub fn token_at(line: &str, cursor: usize) -> Option<(HeaderToken, &str)> {
    header_tokens(line)
        .into_iter()
        .find(|token| cursor >= token.start && cursor <= token.start + token.len)
        .map(|token| {
            let text = &line[token.start..token.start + token.len];
            (token, text)
        })
}

/// A left-to-right reader of a header. Each step that runs out of text answers with the [`Place`] it ran out in.
struct Scanner<'a> {
    bytes: &'a [u8],
    at: usize,
    tokens: Vec<HeaderToken>,
}

impl<'a> Scanner<'a> {
    fn new(line: &'a str, from: usize) -> Self {
        Self {
            bytes: line.as_bytes(),
            at: from,
            tokens: Vec::new(),
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn skip_whitespace(&mut self) {
        while self.peek().is_some_and(|b| b.is_ascii_whitespace()) {
            self.at += 1;
        }
    }

    fn follows_whitespace(&self) -> bool {
        self.at > 0 && self.bytes[self.at - 1].is_ascii_whitespace()
    }

    fn name(&mut self) -> usize {
        let start = self.at;
        while self
            .peek()
            .is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        {
            self.at += 1;
        }
        start
    }

    fn push(&mut self, kind: TokenKind, start: usize) {
        if self.at > start {
            self.tokens.push(HeaderToken {
                kind,
                start,
                len: self.at - start,
            });
        }
    }

    fn text(&self, start: usize) -> String {
        String::from_utf8_lossy(&self.bytes[start..self.at]).into_owned()
    }

    fn header(&mut self) -> Result<(), Place> {
        self.skip_whitespace();
        match self.peek() {
            None => return Err(Place::Elsewhere),
            Some(b'"') => self.string()?,
            Some(_) => {}
        }
        loop {
            self.skip_whitespace();
            match self.peek() {
                None if self.follows_whitespace() => return Err(Place::OptionKey),
                None => return Err(Place::Elsewhere),
                Some(b']') => {
                    self.at += 1;
                    return Ok(());
                }
                Some(_) => self.option()?,
            }
        }
    }

    fn option(&mut self) -> Result<(), Place> {
        let start = self.name();
        let key = self.text(start);
        match self.peek() {
            None => Err(Place::OptionKey),
            Some(b'(') if key == "args" => {
                self.push(TokenKind::OptionKey, start);
                self.at += 1;
                self.list(TokenKind::ArgName, Place::ArgName, |_| Place::ArgValue)
            }
            Some(b':') => {
                self.push(TokenKind::OptionKey, start);
                self.at += 1;
                if key == "matrix" && self.peek() == Some(b'(') {
                    self.at += 1;
                    return self.list(TokenKind::MatrixAxis, Place::MatrixAxis, |axis| {
                        Place::MatrixAxisValue(axis.to_string())
                    });
                }
                self.value(Place::OptionValue(key))
            }
            Some(b) if b.is_ascii_whitespace() || b == b']' => {
                self.push(TokenKind::OptionKey, start);
                Ok(())
            }
            Some(_) => {
                if self.at == start {
                    self.at += 1;
                }
                self.value(Place::Elsewhere)
            }
        }
    }

    /// The `name:value` pairs of an `args(…)` or an inline matrix, up to and past the closing paren.
    fn list(
        &mut self,
        kind: TokenKind,
        at_name: Place,
        in_value: impl Fn(&str) -> Place,
    ) -> Result<(), Place> {
        loop {
            self.skip_whitespace();
            match self.peek() {
                None => return Err(at_name),
                Some(b')') => {
                    self.at += 1;
                    return Ok(());
                }
                Some(_) => {}
            }
            let start = self.name();
            let name = self.text(start);
            match self.peek() {
                None => return Err(at_name),
                Some(b':') => {
                    self.push(kind, start);
                    self.at += 1;
                    self.value(in_value(&name))?;
                }
                Some(_) if self.at == start => self.at += 1,
                Some(_) => self.push(kind, start),
            }
        }
    }

    /// A value, up to the space, `)` or `]` that ends it outside every bracket and string.
    fn value(&mut self, place: Place) -> Result<(), Place> {
        let mut depth = 0usize;
        loop {
            let Some(b) = self.peek() else {
                return Err(place);
            };
            match b {
                b'"' => self.string()?,
                b'(' | b'[' | b'{' => {
                    depth += 1;
                    self.at += 1;
                }
                b')' | b']' | b'}' if depth == 0 => return Ok(()),
                b')' | b']' | b'}' => {
                    depth -= 1;
                    self.at += 1;
                }
                _ if b.is_ascii_whitespace() && depth == 0 => return Ok(()),
                _ => self.at += 1,
            }
        }
    }

    fn string(&mut self) -> Result<(), Place> {
        self.at += 1;
        loop {
            match self.peek() {
                None => return Err(Place::Elsewhere),
                Some(b'\\') => self.at += 2,
                Some(b'"') => {
                    self.at += 1;
                    return Ok(());
                }
                Some(_) => self.at += 1,
            }
        }
    }
}

#[cfg(test)]
#[path = "preview_header_test.rs"]
mod tests;
