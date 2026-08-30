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

    /// Parses one view node (comment, element, if, for, match, or let) starting at `indent`.
    fn parse_view_node(&mut self, indent: usize) -> Result<ViewNode, ParseError> {
        let content = self.lines[self.pos].content.clone();
        let first_word = content.split_whitespace().next().unwrap_or("");

        // Before the tag dispatch: an unrecognised first word becomes a component call, so a note left here
        // used to compile into a call to a component named `//`.
        if content.starts_with("//") {
            self.pos += 1;
            return Ok(ViewNode::Comment(content));
        }

        match first_word {
            "if" => self.parse_if_block(indent),
            "for" => self.parse_for_block(indent),
            "match" => self.parse_match_block(indent),
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
            let mut words = next.content.split_whitespace();
            words.next();
            if words.next() == Some("if") {
                // `else if cond` desugars to the nesting a reader would otherwise write by hand: strip the `else`, re-parse the rest of the line as an `if`, and make it this else-branch's only child. `content_start` moves with the text so the condition still maps to its own bytes.
                let line = &mut self.lines[self.pos];
                let stripped = line.content["else".len()..].trim_start().to_string();
                line.content_start += line.content.len() - stripped.len();
                line.content = stripped;
                else_branch = Some(vec![self.parse_if_block(indent)?]);
            } else {
                self.pos += 1;
                else_branch = Some(self.parse_children(indent)?);
            }
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

        // Split on the first standalone ` in ` keyword, then peel off the optional trailing clauses.
        let header = split_for_in(&rest).ok_or_else(|| ParseError {
            message: format!(
                "expected `for <pattern> in <expr> [key <expr>] [gap:<expr>] [virtual row_height:<expr>]`, got `for {rest}`"
            ),
            line: number,
        })?;

        let body = self.parse_children(indent)?;

        Ok(ViewNode::ForBlock(ForBlock {
            pattern: header.pattern,
            iterable: header.iterable,
            key_expr: header.key_expr,
            gap_expr: header.gap_expr,
            virtual_row_height: header.virtual_row_height,
            body,
            line: number,
        }))
    }

    /// Parses `match <scrutinee> [as <name>] [key <expr>]` and its arms. Every direct child of a `match` is an
    /// arm header — a raw Rust pattern — with the nodes it renders indented under it.
    fn parse_match_block(&mut self, indent: usize) -> Result<ViewNode, ParseError> {
        let line = &self.lines[self.pos];
        let number = line.number;
        let after = &line.content["match".len()..];
        let header_start =
            line.content_start + "match".len() + (after.len() - after.trim_start().len());
        let header = after.trim().to_string();
        self.pos += 1;

        let (scrutinee, binding, key_expr) =
            split_match_header(&header).ok_or_else(|| ParseError {
                message: format!(
                    "expected `match <expr> [as <name>] [key <expr>]`, got `match {header}`"
                ),
                line: number,
            })?;

        let mut arms = Vec::new();
        loop {
            self.skip_blank_view_lines();
            let Some(next) = self.lines.get(self.pos) else {
                break;
            };
            if next.section != Section::View || next.indent <= indent {
                break;
            }
            let arm_indent = next.indent;
            let pattern = next.content.trim().to_string();
            let pattern_start = next.content_start;
            let arm_line = next.number;
            self.pos += 1;
            let body = self.parse_children(arm_indent)?;
            arms.push(MatchArm {
                pattern,
                body,
                line: arm_line,
                pattern_start,
            });
        }

        if arms.is_empty() {
            return Err(ParseError {
                message: "a `match` needs at least one arm".to_string(),
                line: number,
            });
        }

        Ok(ViewNode::MatchBlock(MatchBlock {
            scrutinee,
            binding,
            key_expr,
            arms,
            line: number,
            scrutinee_start: header_start,
        }))
    }

    /// Parses an element line plus its deeper-indented children.
    fn parse_element(&mut self, indent: usize) -> Result<ViewNode, ParseError> {
        let line = &self.lines[self.pos];
        let number = line.number;
        let content_start = line.content_start;
        let mut content = line.content.clone();
        let mut end = content_start + content.len();
        self.pos += 1;

        // An element header runs past its line whenever a value's delimiters are still open — a closure
        // written where it is used, rather than bound in `[logic]` and referred to by name.
        //
        // The join reproduces the source byte for byte: the gap between one line's content and the next's
        // becomes a newline and spaces of exactly the width it occupied. Every span the parser hands the
        // transpiler therefore still points at the `.rsx` byte the author typed, which is what keeps a
        // diagnostic on the right column when the expression it names started three lines up.
        while unclosed_delimiters(&content) {
            let Some(next) = self.lines.get(self.pos) else {
                break;
            };
            if next.section != Section::View {
                break;
            }
            let gap = next.content_start.saturating_sub(end);
            content.push('\n');
            content.push_str(&" ".repeat(gap.saturating_sub(1)));
            content.push_str(&next.content);
            end = next.content_start + next.content.len();
            self.pos += 1;
        }

        let mut element = parse_element_header(&content, number, content_start)?;

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
fn split_for_in(rest: &str) -> Option<ForHeader> {
    let tokens: Vec<&str> = rest.split_whitespace().collect();
    let in_idx = tokens.iter().position(|&t| t == "in")?;
    if in_idx == 0 || in_idx + 1 >= tokens.len() {
        return None;
    }
    let pattern = tokens[..in_idx].join(" ");
    let mut after_in: Vec<&str> = tokens[in_idx + 1..].to_vec();

    // `virtual row_height:<expr>` peels off first: both its tokens sit at the end, and leaving them in would
    // put `virtual` inside the iterable expression.
    let virtual_row_height = match after_in
        .iter()
        .position(|&t| t == "virtual")
        .filter(|at| *at > 0)
    {
        Some(at) => {
            let height = after_in
                .get(at + 1)
                .and_then(|t| split_once_colon(t))
                .filter(|(name, _)| *name == "row_height")
                .map(|(_, value)| value.to_string());
            after_in.truncate(at);
            Some(height?)
        }
        None => None,
    };

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
    Some(ForHeader {
        pattern,
        iterable,
        key_expr,
        gap_expr,
        virtual_row_height,
    })
}

/// The parsed pieces of a `for` header line.
struct ForHeader {
    pattern: String,
    iterable: String,
    key_expr: Option<String>,
    gap_expr: Option<String>,
    virtual_row_height: Option<String>,
}

/// Splits `<scrutinee> [as <name>] [key <expr>]` into its three parts. `key` is peeled first so an `as` inside
/// the key expression cannot be mistaken for the binding.
fn split_match_header(rest: &str) -> Option<(String, Option<String>, Option<String>)> {
    let tokens: Vec<&str> = rest.split_whitespace().collect();
    if tokens.is_empty() {
        return None;
    }
    let (head, key_expr) = match tokens.iter().position(|&t| t == "key") {
        Some(at) if at > 0 && at + 1 < tokens.len() => {
            (&tokens[..at], Some(tokens[at + 1..].join(" ")))
        }
        _ => (&tokens[..], None),
    };
    let (scrutinee, binding) = match head.iter().position(|&t| t == "as") {
        Some(at) if at > 0 && at + 1 == head.len() - 1 => {
            (head[..at].join(" "), Some(head[at + 1].to_string()))
        }
        _ => (head.join(" "), None),
    };
    (!scrutinee.is_empty()).then_some((scrutinee, binding, key_expr))
}

/// Parses an element header line into tag, classes, attrs, and quoted content.
///
/// Tokens are consumed left to right. A bare `@name` is a class; a quoted string is the content; `key:value`
/// is an attribute. Two rules delimit a value and there are no exceptions to them: `key:value` is one token
/// with its parens balanced, and `key(…)` is anything wanting a space at depth 0. Which of the two was
/// written — and whether the text was quoted or an i18n key — is what the resulting [`Value`] records.
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
        children: Vec::new(),
        line,
        content_start,
        content_i18n: false,
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

        if let Some((str_at, is_key)) = string_start(&chars, i) {
            let (text, next, content_at) =
                read_string_value(&chars, str_at).ok_or_else(|| ParseError {
                    message: "unterminated string literal".to_string(),
                    line,
                })?;
            element.content = Some(text);
            element.content_start = content_start + byte_at(&chars, content_at);
            element.content_i18n = is_key;
            i = next;
            continue;
        }

        let token_start = i;
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
            // `key(expr)`: the form for a value wanting a space, delimited by balanced parens so it does not run to end of line and attribute order stops mattering — `on_press(|| f())` and `transition(fill 250ms ease-out)` sit on one line in any order.
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
                value: Value::Spec(value.trim().to_string()),
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
                // Closures take the parenthesized form only: `on_press(|| …)`, never `on_press:|| …`. The colon form ran to end of line and silently swallowed any attribute after it, so it is rejected — styles/values keep the colon form (`fill:red`), closures do not.
                let key = key.trim();
                return Err(ParseError {
                    message: format!(
                        "closure attribute `{key}` must use the parenthesized form `{key}(|…| …)`, not `{key}:|…| …` (the colon form runs to end of line and would swallow the attributes after it)"
                    ),
                    line,
                });
            }

            let mut k = val_start;
            // Allow quoted attribute values, escaped (`"…"`), raw (`r"…"`), or an i18n key (`t"…"`).
            if let Some((str_at, is_key)) = string_start(&chars, k) {
                let (text, next, content_at) =
                    read_string_value(&chars, str_at).ok_or_else(|| ParseError {
                        message: "unterminated string literal in attribute value".to_string(),
                        line,
                    })?;
                element.attributes.push(Attr {
                    key: key.trim().to_string(),
                    value: if is_key {
                        Value::I18n(text)
                    } else {
                        Value::Quoted(text)
                    },
                    value_start: content_start + byte_at(&chars, content_at),
                });
                i = next;
                continue;
            }
            // A colon value runs to the next whitespace, but not one nested inside `(...)`/`[...]`: so a
            // computed value like `fill:chip_fill($snap, id)` (spaces inside the call) is read whole, while a
            // following attribute on the same line still starts after the depth-0 space. Unbalanced parens
            // read to end of line, leaving the malformed expression for the emitter/rustc to reject.
            let mut depth = 0i32;
            while k < len {
                let c = chars[k];
                if c.is_whitespace() && depth == 0 {
                    break;
                }
                match c {
                    '(' | '[' => depth += 1,
                    ')' | ']' => depth -= 1,
                    _ => {}
                }
                k += 1;
            }
            let value: String = chars[val_start..k].iter().collect();
            check_hex_value(&value, line)?;
            element.attributes.push(Attr {
                key: key.trim().to_string(),
                value: Value::Bare(value),
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
                value: Value::Flag,
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

/// Detects a string value starting at `i`: a plain `"…"`, raw `r"…"`, or an i18n key `t"…"`. Returns the index
/// [`read_string_value`] should read from (past any `t` prefix) and whether it is an i18n key. `None` when no
/// string begins at `i`. Only `t` *immediately* followed by a quote is a key marker, so a tag/token like `text`
/// is never mistaken for one.
pub(super) fn string_start(chars: &[char], i: usize) -> Option<(usize, bool)> {
    let is_str = |k: usize| {
        chars.get(k) == Some(&'"') || (chars.get(k) == Some(&'r') && chars.get(k + 1) == Some(&'"'))
    };
    if is_str(i) {
        Some((i, false))
    } else if chars.get(i) == Some(&'t') && is_str(i + 1) {
        Some((i + 1, true))
    } else {
        None
    }
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
/// Whether `content` leaves a bracket open, so the element header continues on the next line. String
/// literals are skipped, since a bracket inside one is text rather than structure.
fn unclosed_delimiters(content: &str) -> bool {
    let (mut depth, mut in_str, mut escaped) = (0i32, false, false);
    for c in content.chars() {
        if in_str {
            match (escaped, c) {
                (true, _) => escaped = false,
                (false, '\\') => escaped = true,
                (false, '"') => in_str = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            _ => {}
        }
    }
    depth > 0
}

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
