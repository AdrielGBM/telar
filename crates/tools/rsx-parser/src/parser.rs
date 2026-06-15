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
        let logic = self.parse_logic();
        let style = self.parse_style()?;
        let view = self.parse_view()?;
        Ok(RsxDocument { logic, style, view })
    }

    // ------------------------------------------------------------------
    // Logic zone
    // ------------------------------------------------------------------

    /// Captures consecutive logic lines verbatim (blanks and comments preserved).
    fn parse_logic(&mut self) -> LogicZone {
        let mut raws = Vec::new();
        while let Some(line) = self.lines.get(self.pos) {
            if line.section != Section::Logic {
                break;
            }
            raws.push(line.raw.clone());
            self.pos += 1;
        }

        // Trim leading/trailing blank lines but keep interior formatting intact.
        let start = raws.iter().position(|l| !l.trim().is_empty()).unwrap_or(0);
        let end = raws
            .iter()
            .rposition(|l| !l.trim().is_empty())
            .map(|i| i + 1)
            .unwrap_or(0);
        let source = if start < end {
            raws[start..end].join("\n")
        } else {
            String::new()
        };

        LogicZone { source }
    }

    // ------------------------------------------------------------------
    // Style section
    // ------------------------------------------------------------------

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

            if line.content.starts_with('.') {
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

    /// Parses a single `name: value` constant line.
    fn parse_style_const(&self) -> Result<StyleConst, ParseError> {
        let line = &self.lines[self.pos];
        let (name, value) = split_once_colon(&line.content).ok_or_else(|| ParseError {
            message: format!(
                "expected `name: value` in style constant, got `{}`",
                line.content
            ),
            line: line.number,
        })?;

        let name = name.trim().to_string();
        let value = parse_style_value(value.trim());

        Ok(StyleConst {
            name,
            value,
            line: line.number,
        })
    }

    /// Parses either an inline `.class: k:v k:v` or a multi-line `.class` with indented props.
    fn parse_style_class(&mut self) -> Result<StyleClass, ParseError> {
        let header = &self.lines[self.pos];
        let header_line = header.number;
        let header_indent = header.indent;
        let after_dot = header.content[1..].to_string();
        self.pos += 1;

        // Inline form: `.badge: padding-x:6 padding-y:2 radius:6`
        if let Some((name, rest)) = split_once_colon(&after_dot) {
            let name = name.trim().to_string();
            let props = parse_inline_props(rest.trim(), header_line)?;
            return Ok(StyleClass {
                name,
                props,
                line: header_line,
            });
        }

        // Multi-line form: name is the rest of the header, props are indented lines.
        let name = after_dot.trim().to_string();
        if name.is_empty() {
            return Err(ParseError {
                message: "style class is missing a name after `.`".to_string(),
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
            if line.content.starts_with('.') {
                break;
            }

            let (key, value) = split_once_colon(&line.content).ok_or_else(|| ParseError {
                message: format!(
                    "expected `key: value` in style class `{name}`, got `{}`",
                    line.content
                ),
                line: line.number,
            })?;
            props.push(StyleProp {
                key: key.trim().to_string(),
                value: value.trim().to_string(),
            });
            self.pos += 1;
        }

        Ok(StyleClass {
            name,
            props,
            line: header_line,
        })
    }

    // ------------------------------------------------------------------
    // View section
    // ------------------------------------------------------------------

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
            // A deeper line without a preceding parent at this level is unexpected;
            // treat it as belonging to the previous sibling's children (handled by recursion),
            // so at this point indent must equal min_indent.
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
        let line = &self.lines[self.pos];
        let content = line.content.clone();
        let number = line.number;
        let first_word = content.split_whitespace().next().unwrap_or("");

        match first_word {
            "if" => self.parse_if_block(indent),
            "for" => self.parse_for_block(indent),
            "let" => {
                self.pos += 1;
                Ok(ViewNode::LetStmt {
                    source: content,
                    line: number,
                })
            }
            _ => self.parse_element(indent),
        }
    }

    fn parse_if_block(&mut self, indent: usize) -> Result<ViewNode, ParseError> {
        let line = &self.lines[self.pos];
        let number = line.number;
        let condition = line.content["if".len()..].trim().to_string();
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
        }))
    }

    fn parse_for_block(&mut self, indent: usize) -> Result<ViewNode, ParseError> {
        let line = &self.lines[self.pos];
        let number = line.number;
        let rest = line.content["for".len()..].trim().to_string();
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
        }))
    }

    /// Parses an element line plus its deeper-indented children.
    fn parse_element(&mut self, indent: usize) -> Result<ViewNode, ParseError> {
        let line = &self.lines[self.pos];
        let number = line.number;
        let content = line.content.clone();
        self.pos += 1;

        let mut element = parse_element_header(&content, number)?;

        // `canvas` may declare drawing-area closure params (`|w, h|`) before its children.
        if element.tag == "canvas" {
            self.skip_blank_view_lines();
            if let Some(next) = self.lines.get(self.pos)
                && next.section == Section::View
                && next.indent > indent
                && next.content.starts_with('|')
                && let Some(params) = parse_canvas_params(&next.content)
            {
                element.canvas_params = Some(params);
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
}

// ----------------------------------------------------------------------
// Free helpers
// ----------------------------------------------------------------------

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

/// Parses inline class props: `padding-x:6  padding-y:2  radius:6`.
fn parse_inline_props(s: &str, line: usize) -> Result<Vec<StyleProp>, ParseError> {
    let mut props = Vec::new();
    for token in s.split_whitespace() {
        let (key, value) = split_once_colon(token).ok_or_else(|| ParseError {
            message: format!("expected `key:value` in inline style class, got `{token}`"),
            line,
        })?;
        props.push(StyleProp {
            key: key.trim().to_string(),
            value: value.trim().to_string(),
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
/// Tokens are consumed left to right. A bare `.name` is a class; a quoted string
/// is the content; `key:value` is an attribute. When an attribute value begins
/// with `||` or `|args|`, the entire remainder of the line becomes that value.
fn parse_element_header(content: &str, line: usize) -> Result<Element, ParseError> {
    let mut element = Element {
        tag: String::new(),
        classes: Vec::new(),
        attrs: Vec::new(),
        content: None,
        canvas_params: None,
        children: Vec::new(),
        line,
    };

    let chars: Vec<char> = content.chars().collect();
    let mut i = 0;
    let len = chars.len();
    let mut first = true;

    while i < len {
        // Skip spaces between tokens.
        while i < len && chars[i].is_whitespace() {
            i += 1;
        }
        if i >= len {
            break;
        }

        // Quoted string → element content.
        if chars[i] == '"' {
            let (text, next) = read_quoted(&chars, i).ok_or_else(|| ParseError {
                message: "unterminated string literal".to_string(),
                line,
            })?;
            element.content = Some(text);
            i = next;
            continue;
        }

        // Read one whitespace-delimited token, but be aware it may contain a closure value.
        let token_start = i;
        // First find where the `key:` part ends (if any) to detect closure values.
        // We scan the token up to the first whitespace, while watching for a `:` that
        // introduces a closure value spanning the rest of the line.
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
            // Examine the value start right after the colon.
            let val_start = colon + 1;
            let value_is_closure = chars.get(val_start) == Some(&'|');

            if value_is_closure {
                // The closure value runs to the end of the line, verbatim.
                let value: String = chars[val_start..].iter().collect();
                element.attrs.push(Attr {
                    key: key.trim().to_string(),
                    value: value.trim().to_string(),
                });
                break;
            }

            // Plain attribute value: up to the next whitespace.
            let mut k = val_start;
            // Allow quoted attribute values.
            if chars.get(k) == Some(&'"') {
                let (text, next) = read_quoted(&chars, k).ok_or_else(|| ParseError {
                    message: "unterminated string literal in attribute value".to_string(),
                    line,
                })?;
                element.attrs.push(Attr {
                    key: key.trim().to_string(),
                    value: text,
                });
                i = next;
                continue;
            }
            while k < len && !chars[k].is_whitespace() {
                k += 1;
            }
            let value: String = chars[val_start..k].iter().collect();
            element.attrs.push(Attr {
                key: key.trim().to_string(),
                value,
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

        if let Some(class) = token.strip_prefix('.') {
            element.classes.push(class.to_string());
        } else {
            // A bare token after the tag is a flag-style attribute (e.g. `ghost`).
            element.attrs.push(Attr {
                key: token,
                value: String::new(),
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

/// Reads a double-quoted string starting at index `start` (which must be `"`).
/// Returns the unescaped-free inner text (escapes kept verbatim) and the index past the closing quote.
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
