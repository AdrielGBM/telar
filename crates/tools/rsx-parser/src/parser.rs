//! Recursive-descent parser for `.rsx` documents.

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

    fn parse_style(&mut self) -> Result<StyleSection, ParseError> {
        let mut section = StyleSection::default();

        while let Some(line) = self.lines.get(self.pos) {
            if line.section != Section::Style {
                break;
            }
            if line.is_blank() {
                self.pos += 1;
                continue;
            }

            if line.content.starts_with('@') {
                let class = self.parse_style_class()?;
                section.classes.push(class);
            } else {
                let constant = self.parse_style_const()?;
                section.constants.push(constant);
                self.pos += 1;
            }
        }

        Ok(section)
    }

    fn parse_style_const(&self) -> Result<StyleConstant, ParseError> {
        let line = &self.lines[self.pos];
        let (name, value) = split_once_colon(&line.content).ok_or_else(|| ParseError {
            message: format!(
                "expected `name: value` in style constant, got `{}`",
                line.content
            ),
            line: line.number,
        })?;

        let name = name.trim().to_string();
        let value = value.trim();
        if value.is_empty() {
            return Err(ParseError {
                message: format!("style constant `{name}` is missing a value after `:`"),
                line: line.number,
            });
        }
        check_hex_value(value, line.number)?;

        Ok(StyleConstant {
            name,
            value: parse_style_value(value),
            line: line.number,
        })
    }

    /// Parses either an inline `@class: k:v k:v` or a multi-line `@class` with indented props.
    fn parse_style_class(&mut self) -> Result<StyleClass, ParseError> {
        let header = &self.lines[self.pos];
        let header_line = header.number;
        let header_indent = header.indent;
        // Strip the leading `@` sigil; the rest is the class name (and any inline props).
        let after_sigil = header.content[1..].to_string();
        self.pos += 1;

        // Inline form: `@badge: padding_x:6 padding_y:2 radius:6`
        if let Some((name, rest)) = split_once_colon(&after_sigil) {
            let name = name.trim().to_string();
            let props = parse_inline_props(rest.trim(), header_line)?;
            return Ok(StyleClass {
                name,
                props,
                line: header_line,
            });
        }

        // Multi-line form: name is the rest of the header, props are indented lines.
        let name = after_sigil.trim().to_string();
        if name.is_empty() {
            return Err(ParseError {
                message: "style class is missing a name after `@`".to_string(),
                line: header_line,
            });
        }

        let mut props = Vec::new();
        while let Some(line) = self.lines.get(self.pos) {
            if line.section != Section::Style {
                break;
            }
            if line.is_blank() {
                self.pos += 1;
                continue;
            }
            // A child property must be indented deeper than the class header.
            if line.indent <= header_indent {
                break;
            }
            // A nested class would start a new definition.
            if line.content.starts_with('@') {
                break;
            }

            let (key, value) = split_once_colon(&line.content).ok_or_else(|| ParseError {
                message: format!(
                    "expected `key: value` in style class `{name}`, got `{}`",
                    line.content
                ),
                line: line.number,
            })?;
            let (key, value) = (key.trim(), value.trim());
            check_style_prop_value(key, value, line.number)?;
            props.push(StyleProp {
                key: key.to_string(),
                value: value.to_string(),
            });
            self.pos += 1;
        }

        Ok(StyleClass {
            name,
            props,
            line: header_line,
        })
    }

    fn parse_view(&mut self) -> Result<ViewSection, ParseError> {
        // Root nodes live at the smallest indentation present in the view.
        let base_indent = self
            .lines
            .iter()
            .skip(self.pos)
            .filter(|l| l.section == Section::View && !l.is_blank())
            .map(|l| l.indent)
            .min()
            .unwrap_or(0);

        let nodes = self.parse_view_nodes(base_indent)?;

        // Any view line left unconsumed here is stranded at an indentation that lines up with no
        // enclosing block (e.g. a child indented one space deeper than its siblings). Erroring is
        // what keeps the formatter from silently dropping it when it re-serializes the AST.
        self.skip_blank_view_lines();
        if let Some(line) = self.lines.get(self.pos)
            && line.section == Section::View
            && !line.is_blank()
        {
            return Err(ParseError {
                line: line.number,
                message:
                    "inconsistent indentation: this line does not line up with any enclosing element"
                        .into(),
            });
        }

        Ok(ViewSection { nodes })
    }

    /// Parses all sibling nodes at exactly `min_indent`, recursing into deeper children.
    fn parse_view_nodes(&mut self, min_indent: usize) -> Result<Vec<ViewNode>, ParseError> {
        let mut nodes = Vec::new();

        loop {
            self.skip_blank_view_lines();
            let Some(line) = self.lines.get(self.pos) else {
                break;
            };
            if line.section != Section::View {
                break;
            }
            // Dedent: this line belongs to an ancestor, stop collecting siblings here.
            if line.indent < min_indent {
                break;
            }
            // Indent exceeds min_indent: children are consumed by recursion, so this means over-indentation; stop the sibling scan.
            if line.indent > min_indent {
                break;
            }

            let node = self.parse_view_node(min_indent)?;
            nodes.push(node);
        }

        Ok(nodes)
    }

    /// Parses one view node (element, if, for, or let) starting at `indent`.
    fn parse_view_node(&mut self, indent: usize) -> Result<ViewNode, ParseError> {
        let content = self.lines[self.pos].content.clone();
        let first_word = content.split_whitespace().next().unwrap_or("");

        match first_word {
            "if" => self.parse_if_block(indent),
            "for" => self.parse_for_block(indent),
            "let" => {
                let source_start = self.lines[self.pos].content_start;
                self.pos += 1;
                Ok(ViewNode::LetStmt {
                    source: content,
                    source_start,
                })
            }
            _ => self.parse_element(indent),
        }
    }

    fn parse_if_block(&mut self, indent: usize) -> Result<ViewNode, ParseError> {
        let line = &self.lines[self.pos];
        let number = line.number;
        let after = &line.content["if".len()..];
        let condition_start =
            line.content_start + "if".len() + (after.len() - after.trim_start().len());
        let condition = after.trim().to_string();
        self.pos += 1;

        let then_branch = self.parse_children(indent)?;

        // Optional `else` at the same indentation as the `if`.
        let mut else_branch = None;
        self.skip_blank_view_lines();
        if let Some(next) = self.lines.get(self.pos)
            && next.section == Section::View
            && next.indent == indent
            && next.content.split_whitespace().next() == Some("else")
        {
            self.pos += 1;
            else_branch = Some(self.parse_children(indent)?);
        }

        Ok(ViewNode::IfBlock(IfBlock {
            condition,
            then_branch,
            else_branch,
            line: number,
            condition_start,
        }))
    }

    fn parse_for_block(&mut self, indent: usize) -> Result<ViewNode, ParseError> {
        let line = &self.lines[self.pos];
        let number = line.number;
        let after = &line.content["for".len()..];
        // Start of the `<pattern> in <expr>` body in source; `split_for_in` re-tokenizes it, so the
        // resulting pattern/iterable are not verbatim substrings and no expression span is emitted.
        let rest_start =
            line.content_start + "for".len() + (after.len() - after.trim_start().len());
        let rest = after.trim().to_string();
        self.pos += 1;

        // Split on the first standalone ` in ` keyword.
        let (pattern, iterable) = split_for_in(&rest).ok_or_else(|| ParseError {
            message: format!("expected `for <pattern> in <expr>`, got `for {rest}`"),
            line: number,
        })?;

        let body = self.parse_children(indent)?;

        Ok(ViewNode::ForBlock(ForBlock {
            pattern,
            iterable,
            body,
            line: number,
            pattern_start: rest_start,
            iterable_start: rest_start,
        }))
    }

    /// Parses an element line plus its deeper-indented children.
    fn parse_element(&mut self, indent: usize) -> Result<ViewNode, ParseError> {
        let line = &self.lines[self.pos];
        let number = line.number;
        let content = line.content.clone();
        let content_start = line.content_start;
        self.pos += 1;

        let mut element = parse_element_header(&content, number, content_start)?;

        // `canvas` may declare drawing-area closure params (`|w, h|`) before its children.
        if element.tag == "canvas" {
            self.skip_blank_view_lines();
            if let Some(next) = self.lines.get(self.pos)
                && next.section == Section::View
                && next.indent > indent
                && next.content.starts_with('|')
                && let Some(params) = parse_canvas_params(&next.content)
            {
                element.canvas_parameters = Some(params);
                self.pos += 1;
            }
        }

        element.children = self.parse_children(indent)?;

        Ok(ViewNode::Element(element))
    }

    /// Collects the children block of a node opened at `parent_indent`.
    fn parse_children(&mut self, parent_indent: usize) -> Result<Vec<ViewNode>, ParseError> {
        self.skip_blank_view_lines();
        let Some(next) = self.lines.get(self.pos) else {
            return Ok(Vec::new());
        };
        if next.section != Section::View || next.indent <= parent_indent {
            return Ok(Vec::new());
        }
        let child_indent = next.indent;
        self.parse_view_nodes(child_indent)
    }

    fn skip_blank_view_lines(&mut self) {
        while let Some(line) = self.lines.get(self.pos) {
            if line.section == Section::View && line.is_blank() {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    /// Parses the trailing `[preview "Name" …]` sections. Each header is a `Section::Preview` line;
    /// its body is the following `Section::View` markup, parsed with the same view machinery, up to
    /// the next preview header or EOF.
    fn parse_previews(&mut self) -> Result<Vec<Preview>, ParseError> {
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
) -> Result<(String, Vec<(String, String)>), ParseError> {
    let inner = content
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .ok_or_else(|| ParseError {
            message: "malformed preview header: expected `[preview \"name\" …]`".to_string(),
            line,
        })?;
    let rest = inner
        .trim()
        .strip_prefix("preview")
        .ok_or_else(|| ParseError {
            message: "preview header must start with `preview`".to_string(),
            line,
        })?
        .trim_start();

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
            Some((k, v)) => options.push((k.trim().to_string(), v.trim().to_string())),
            None => options.push((token.to_string(), String::new())),
        }
    }
    Ok((name, options))
}

/// Splits a string on its first `:` that is not part of a closure/`::` path.
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

/// A hex color body (the part after `#`) is valid only at the lengths the transpiler expands:
/// `#rgb`, `#rrggbb`, `#rrggbbaa`, all hex digits.
fn is_valid_hex(hex: &str) -> bool {
    matches!(hex.len(), 3 | 6 | 8) && hex.bytes().all(|b| b.is_ascii_hexdigit())
}

/// `#` is reserved for hex colors in `.rsx`, so any `#`-prefixed value must be a well-formed hex.
/// Catches typos (`#zzz`, `#12`) at parse time instead of silently rendering them as black.
fn check_hex_value(value: &str, line: usize) -> Result<(), ParseError> {
    if let Some(hex) = value.strip_prefix('#')
        && !is_valid_hex(hex)
    {
        return Err(ParseError {
            message: format!("invalid hex color `{value}`: expected #rgb, #rrggbb or #rrggbbaa"),
            line,
        });
    }
    Ok(())
}

/// Validates a style-class property `key: value`: the value must be present and, when it is a hex
/// color, well-formed.
fn check_style_prop_value(key: &str, value: &str, line: usize) -> Result<(), ParseError> {
    if value.is_empty() {
        return Err(ParseError {
            message: format!("style property `{key}` is missing a value after `:`"),
            line,
        });
    }
    check_hex_value(value, line)
}

/// Classifies a style constant value.
fn parse_style_value(raw: &str) -> StyleValue {
    if let Some(hex) = raw.strip_prefix('#') {
        return StyleValue::Hex(format!("#{hex}"));
    }
    if let Ok(n) = raw.parse::<f32>() {
        return StyleValue::Number(n);
    }
    StyleValue::Raw(raw.to_string())
}

/// Parses inline class props: `padding_x:6  padding_y:2  radius:6`.
fn parse_inline_props(s: &str, line: usize) -> Result<Vec<StyleProp>, ParseError> {
    let mut props = Vec::new();
    for token in s.split_whitespace() {
        let (key, value) = split_once_colon(token).ok_or_else(|| ParseError {
            message: format!("expected `key:value` in inline style class, got `{token}`"),
            line,
        })?;
        let (key, value) = (key.trim(), value.trim());
        check_style_prop_value(key, value, line)?;
        props.push(StyleProp {
            key: key.to_string(),
            value: value.to_string(),
        });
    }
    Ok(props)
}

/// Splits a `for` header on the first top-level ` in ` keyword.
fn split_for_in(rest: &str) -> Option<(String, String)> {
    let tokens: Vec<&str> = rest.split_whitespace().collect();
    let in_idx = tokens.iter().position(|&t| t == "in")?;
    if in_idx == 0 || in_idx + 1 >= tokens.len() {
        return None;
    }
    let pattern = tokens[..in_idx].join(" ");
    let iterable = tokens[in_idx + 1..].join(" ");
    Some((pattern, iterable))
}

/// Extracts `w, h` from a `|w, h|` canvas closure-param line.
fn parse_canvas_params(content: &str) -> Option<String> {
    let after = content.strip_prefix('|')?;
    let end = after.find('|')?;
    Some(after[..end].trim().to_string())
}

/// Parses an element header line into tag, classes, attrs, and quoted content.
///
/// Tokens are consumed left to right. A bare `@name` is a class; a quoted string
/// is the content; `key:value` is an attribute. When an attribute value begins
/// with `||` or `|args|`, the entire remainder of the line becomes that value.
fn parse_element_header(
    content: &str,
    line: usize,
    content_start: usize,
) -> Result<Element, ParseError> {
    let mut element = Element {
        tag: String::new(),
        classes: Vec::new(),
        attributes: Vec::new(),
        content: None,
        canvas_parameters: None,
        children: Vec::new(),
        line,
        content_start,
    };

    let chars: Vec<char> = content.chars().collect();
    let mut i = 0;
    let len = chars.len();
    let mut first = true;

    while i < len {
        while i < len && chars[i].is_whitespace() {
            i += 1;
        }
        if i >= len {
            break;
        }

        if chars[i] == '"' {
            let (text, next) = read_quoted(&chars, i).ok_or_else(|| ParseError {
                message: "unterminated string literal".to_string(),
                line,
            })?;
            element.content = Some(text);
            // Content starts one char past the opening quote.
            element.content_start = content_start + byte_at(&chars, i + 1);
            i = next;
            continue;
        }

        // Read one whitespace-delimited token, but be aware it may contain a closure value.
        let token_start = i;
        // First find where the `key:` part ends (if any) to detect closure values. We scan the token up to the first whitespace, while watching for a `:` that introduces a closure value spanning the rest of the line.
        let mut j = i;
        let mut colon_at: Option<usize> = None;
        while j < len && !chars[j].is_whitespace() {
            if chars[j] == ':' {
                // Ignore `::` path separators.
                if chars.get(j + 1) == Some(&':') {
                    j += 2;
                    continue;
                }
                colon_at = Some(j);
                break;
            }
            j += 1;
        }

        if let Some(colon) = colon_at {
            let key: String = chars[token_start..colon].iter().collect();
            let val_start = colon + 1;
            let is_closure_value = chars.get(val_start) == Some(&'|');

            if is_closure_value {
                // The closure value runs to the end of the line, verbatim, starting at `val_start`.
                let value: String = chars[val_start..].iter().collect();
                element.attributes.push(Attr {
                    key: key.trim().to_string(),
                    value: value.trim().to_string(),
                    is_quoted: false,
                    value_start: content_start + byte_at(&chars, val_start),
                });
                break;
            }

            let mut k = val_start;
            // Allow quoted attribute values.
            if chars.get(k) == Some(&'"') {
                let (text, next) = read_quoted(&chars, k).ok_or_else(|| ParseError {
                    message: "unterminated string literal in attribute value".to_string(),
                    line,
                })?;
                element.attributes.push(Attr {
                    key: key.trim().to_string(),
                    value: text,
                    is_quoted: true,
                    // Value starts one char past the opening quote.
                    value_start: content_start + byte_at(&chars, k + 1),
                });
                i = next;
                continue;
            }
            while k < len && !chars[k].is_whitespace() {
                k += 1;
            }
            let value: String = chars[val_start..k].iter().collect();
            check_hex_value(&value, line)?;
            element.attributes.push(Attr {
                key: key.trim().to_string(),
                value,
                is_quoted: false,
                value_start: content_start + byte_at(&chars, val_start),
            });
            i = k;
            continue;
        }

        // No colon: it's either the tag, a class, or a bare flag attribute.
        let token: String = chars[token_start..j].iter().collect();
        i = j;

        if first {
            element.tag = token;
            first = false;
            continue;
        }

        if let Some(class) = token.strip_prefix('@') {
            element.classes.push(class.to_string());
        } else {
            // A bare token after the tag is a flag-style attribute (e.g. `ghost`).
            element.attributes.push(Attr {
                key: token,
                value: String::new(),
                is_quoted: false,
                value_start: content_start + byte_at(&chars, token_start),
            });
        }
    }

    if element.tag.is_empty() {
        return Err(ParseError {
            message: "view element is missing a tag".to_string(),
            line,
        });
    }

    Ok(element)
}

/// Byte offset within the original `content` string of the char at index `idx` (sum of the UTF-8
/// widths of the preceding chars). Converts a `Vec<char>` index into a source byte offset.
fn byte_at(chars: &[char], idx: usize) -> usize {
    chars[..idx].iter().map(|c| c.len_utf8()).sum()
}

/// Reads a double-quoted string starting at `start`; returns the inner text with escape sequences preserved verbatim, plus the index past the closing quote.
fn read_quoted(chars: &[char], start: usize) -> Option<(String, usize)> {
    debug_assert_eq!(chars.get(start), Some(&'"'));
    let mut i = start + 1;
    let mut out = String::new();
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' {
            // Keep the escape sequence verbatim so the transpiler can re-emit it.
            out.push(c);
            if let Some(&next) = chars.get(i + 1) {
                out.push(next);
                i += 2;
                continue;
            }
            return None;
        }
        if c == '"' {
            return Some((out, i + 1));
        }
        out.push(c);
        i += 1;
    }
    None
}
