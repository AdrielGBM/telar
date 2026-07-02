//! `[view]` section: the indentation-based element tree (elements, `if`/`for`/`let` nodes).

use super::Parser;
use super::style::check_hex_value;
use crate::ast::*;
use crate::error::ParseError;
use crate::lexer::Section;

impl Parser {
    pub(super) fn parse_view(&mut self) -> Result<ViewSection, ParseError> {
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
    /// `pub(super)` because `preview::parse_preview_body` reuses it to parse a preview's body.
    pub(super) fn parse_view_nodes(
        &mut self,
        min_indent: usize,
    ) -> Result<Vec<ViewNode>, ParseError> {
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

    /// `pub(super)` because `preview::parse_previews`/`parse_preview_body` also skip blank view
    /// lines when scanning for a preview body.
    pub(super) fn skip_blank_view_lines(&mut self) {
        while let Some(line) = self.lines.get(self.pos) {
            if line.section == Section::View && line.is_blank() {
                self.pos += 1;
            } else {
                break;
            }
        }
    }
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

            // A `transition:` value is a space-separated spec (`opacity 200ms ease-out`), optionally comma-separated for several properties, so — like a closure value — it runs verbatim to the end of the line.
            if key.trim() == "transition" {
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
/// `pub(super)` because `preview::parse_preview_header` reads the quoted preview name with it too.
pub(super) fn read_quoted(chars: &[char], start: usize) -> Option<(String, usize)> {
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
