//! `[view]` section: the indentation-based element tree (elements, `if`/`for`/`let` nodes).

use super::style::check_hex_value;
use super::{Parser, split_once_colon};
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
                Ok(ViewNode::LetStmt(LetStmt {
                    source: content,
                    source_start,
                }))
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
        let rest = after.trim().to_string();
        self.pos += 1;

        // Split on the first standalone ` in ` keyword, then peel off optional `key <expr>` / `gap:<expr>` clauses.
        let (pattern, iterable, key_expr, gap_expr) =
            split_for_in(&rest).ok_or_else(|| ParseError {
                message: format!("expected `for <pattern> in <expr>`, got `for {rest}`"),
                line: number,
            })?;

        let body = self.parse_children(indent)?;

        Ok(ViewNode::ForBlock(ForBlock {
            pattern,
            iterable,
            key_expr,
            gap_expr,
            body,
            line: number,
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

        // Any element whose first deeper-indented child is a `|…|` line declares leading closure
        // params before its real children; the transpiler owns the interpretation (e.g. `canvas`
        // drawing-area dimensions `|w, h|`).
        self.skip_blank_view_lines();
        if let Some(next) = self.lines.get(self.pos)
            && next.section == Section::View
            && next.indent > indent
            && next.content.starts_with('|')
            && let Some(params) = parse_leading_params(&next.content)
        {
            element.leading_params = Some(params);
            self.pos += 1;
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

/// Splits a `for` header into `(pattern, iterable, key_expr, gap_expr)`: on the first ` in ` keyword, then
/// an optional trailing ` gap:<expr>` clause (item spacing, reactive-list only), then an optional
/// ` key <expr>` clause (identity for reconciliation; without it a reactive list reconciles by position).
/// `gap` is always the last token on the line, so it's peeled off before the `key` search.
fn split_for_in(rest: &str) -> Option<(String, String, Option<String>, Option<String>)> {
    let tokens: Vec<&str> = rest.split_whitespace().collect();
    let in_idx = tokens.iter().position(|&t| t == "in")?;
    if in_idx == 0 || in_idx + 1 >= tokens.len() {
        return None;
    }
    let pattern = tokens[..in_idx].join(" ");
    let mut after_in: Vec<&str> = tokens[in_idx + 1..].to_vec();

    let gap_expr = match (
        after_in.len(),
        after_in.last().and_then(|t| split_once_colon(t)),
    ) {
        (len, Some((name, value))) if len > 1 && name == "gap" => {
            let value = value.to_string();
            after_in.pop();
            Some(value)
        }
        _ => None,
    };

    let (iterable, key_expr) = match after_in.iter().position(|&t| t == "key") {
        Some(ki) if ki > 0 && ki + 1 < after_in.len() => {
            (after_in[..ki].join(" "), Some(after_in[ki + 1..].join(" ")))
        }
        _ => (after_in.join(" "), None),
    };
    Some((pattern, iterable, key_expr, gap_expr))
}

/// Extracts the inner text from a leading `|params|` line (e.g. `w, h` from `|w, h|`).
fn parse_leading_params(content: &str) -> Option<String> {
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
        leading_params: None,
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

        if chars[i] == '"' || (chars[i] == 'r' && chars.get(i + 1) == Some(&'"')) {
            let (text, next, content_at) =
                read_string_value(&chars, i).ok_or_else(|| ParseError {
                    message: "unterminated string literal".to_string(),
                    line,
                })?;
            element.content = Some(text);
            element.content_start = content_start + byte_at(&chars, content_at);
            i = next;
            continue;
        }

        // Read one whitespace-delimited token, but be aware it may contain a closure value.
        let token_start = i;
        // First find where the `key:` part ends (if any) to detect closure values. We scan the token up to the first whitespace, while watching for a `:` that introduces a closure value spanning the rest of the line.
        let mut j = i;
        let mut colon_at: Option<usize> = None;
        let mut paren_at: Option<usize> = None;
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
            // `key(expr)`: a parenthesized value (closure or spec), delimited by balanced parens so it
            // does not run to end of line and attribute order stops mattering — e.g. `on_press(|| f())`
            // and `transition(fill 250ms ease-out)` can sit on one line in any order.
            if chars[j] == '(' {
                paren_at = Some(j);
                break;
            }
            j += 1;
        }

        if let Some(paren) = paren_at {
            let key: String = chars[token_start..paren].iter().collect();
            let (value, next) = read_balanced_parens(&chars, paren).ok_or_else(|| ParseError {
                message: "unterminated `(` in attribute value".to_string(),
                line,
            })?;
            // Map the value span to the first non-space char inside the parens (matches the colon form).
            let mut vs = paren + 1;
            while vs < len && chars[vs].is_whitespace() {
                vs += 1;
            }
            element.attributes.push(Attr {
                key: key.trim().to_string(),
                value: value.trim().to_string(),
                is_quoted: false,
                value_start: content_start + byte_at(&chars, vs),
            });
            i = next;
            continue;
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
                // A real transition value never has a `:` at paren/bracket depth 0 (durations, easings,
                // and `spring(...)`/`cubic-bezier(...)` keep their colons, if any, nested inside parens).
                // A depth-0 `:` means a trailing attribute got swallowed by the run-to-EOL scan above.
                let mut depth = 0i32;
                for c in value.chars() {
                    match c {
                        '(' | '[' => depth += 1,
                        ')' | ']' => depth -= 1,
                        ':' if depth == 0 => {
                            return Err(ParseError {
                                message: "transition: must be the last attribute on the line; move the trailing attribute(s) before it, or wrap the value as transition(...)".to_string(),
                                line,
                            });
                        }
                        _ => {}
                    }
                }
                element.attributes.push(Attr {
                    key: key.trim().to_string(),
                    value: value.trim().to_string(),
                    is_quoted: false,
                    value_start: content_start + byte_at(&chars, val_start),
                });
                break;
            }

            let mut k = val_start;
            // Allow quoted attribute values, escaped (`"…"`) or raw (`r"…"`).
            if chars.get(k) == Some(&'"')
                || (chars.get(k) == Some(&'r') && chars.get(k + 1) == Some(&'"'))
            {
                let (text, next, content_at) =
                    read_string_value(&chars, k).ok_or_else(|| ParseError {
                        message: "unterminated string literal in attribute value".to_string(),
                        line,
                    })?;
                element.attributes.push(Attr {
                    key: key.trim().to_string(),
                    value: text,
                    is_quoted: true,
                    value_start: content_start + byte_at(&chars, content_at),
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

/// Reads a double-quoted string starting at `start`; interprets the C-style escapes `\"`, `\\`, `\n`,
/// `\t`, `\r`, `\0` into their real characters (so a literal `"` inside content is written `\"`) and
/// returns the decoded text plus the index past the closing quote. An unknown escape (`\d`) keeps both
/// chars verbatim. `pub(super)` because `preview::parse_preview_header` reads the quoted preview name with it too.
pub(super) fn read_quoted(chars: &[char], start: usize) -> Option<(String, usize)> {
    debug_assert_eq!(chars.get(start), Some(&'"'));
    let mut i = start + 1;
    let mut out = String::new();
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' {
            match chars.get(i + 1) {
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('0') => out.push('\0'),
                // Unknown escape: keep the backslash and the following char literally.
                Some(&other) => {
                    out.push('\\');
                    out.push(other);
                }
                // Dangling backslash at end of input: unterminated.
                None => return None,
            }
            i += 2;
            continue;
        }
        if c == '"' {
            return Some((out, i + 1));
        }
        out.push(c);
        i += 1;
    }
    None
}

/// Reads a raw string `r"…"` (`chars[start] == 'r'`, `chars[start + 1] == '"'`): the content is taken
/// verbatim up to the next `"`, with NO escape processing, so `\` is a literal backslash — handy for
/// Windows paths, regexes, or code snippets. A raw string cannot itself contain a `"`; use the escaped
/// form (`"…\"…"`) for that. Returns the content and the index past the closing quote.
pub(super) fn read_raw_quoted(chars: &[char], start: usize) -> Option<(String, usize)> {
    debug_assert_eq!(chars.get(start), Some(&'r'));
    debug_assert_eq!(chars.get(start + 1), Some(&'"'));
    let mut i = start + 2;
    let mut out = String::new();
    while i < chars.len() {
        if chars[i] == '"' {
            return Some((out, i + 1));
        }
        out.push(chars[i]);
        i += 1;
    }
    None
}

/// Reads a string value at `k` — an escaped `"…"` or a raw `r"…"`. Returns `(content, index past it, char
/// index where the content begins)`, the last for source-map offsets. `None` if `k` is not a string start.
pub(super) fn read_string_value(chars: &[char], k: usize) -> Option<(String, usize, usize)> {
    if chars.get(k) == Some(&'"') {
        read_quoted(chars, k).map(|(s, next)| (s, next, k + 1))
    } else if chars.get(k) == Some(&'r') && chars.get(k + 1) == Some(&'"') {
        read_raw_quoted(chars, k).map(|(s, next)| (s, next, k + 2))
    } else {
        None
    }
}

/// Reads a balanced `( … )` group starting at `open` (which must be `(`). Returns the inner text with the
/// outer parens stripped, plus the index past the closing `)`. Nested parens are balanced and parens inside
/// a `"…"` string literal are ignored, so a closure body like `|| f(x)` is captured whole. `None` if unbalanced.
fn read_balanced_parens(chars: &[char], open: usize) -> Option<(String, usize)> {
    debug_assert_eq!(chars.get(open), Some(&'('));
    let mut depth = 0usize;
    let mut in_str = false;
    let mut out = String::new();
    let mut i = open;
    while i < chars.len() {
        let c = chars[i];
        if in_str {
            out.push(c);
            if c == '\\' {
                if let Some(&n) = chars.get(i + 1) {
                    out.push(n);
                    i += 2;
                    continue;
                }
            }
            if c == '"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        match c {
            '"' => {
                in_str = true;
                out.push(c);
            }
            // The outermost `(` and its matching `)` are delimiters, not part of the value.
            '(' => {
                depth += 1;
                if depth > 1 {
                    out.push(c);
                }
            }
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some((out, i + 1));
                }
                out.push(c);
            }
            _ => out.push(c),
        }
        i += 1;
    }
    None
}
