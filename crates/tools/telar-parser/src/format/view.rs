//! The `[view]` and `[preview]` zones, re-emitted from the AST at four spaces a level.

use crate::{Attr, Element, Preview, Value, ViewNode};

use super::INDENT;

pub(super) fn format_view_section(nodes: &[ViewNode]) -> String {
    let mut body = String::new();
    for node in nodes {
        emit_node(node, 0, &mut body);
    }
    let body = body.trim_end();
    if body.is_empty() {
        "[view]".to_string()
    } else {
        format!("[view]\n{body}")
    }
}

pub(super) fn emit_node(node: &ViewNode, depth: usize, out: &mut String) {
    let pad = INDENT.repeat(depth);
    match node {
        ViewNode::Element(element) => {
            out.push_str(&pad);
            out.push_str(&format_element_header(element));
            out.push('\n');
            for child in &element.children {
                emit_node(child, depth + 1, out);
            }
        }
        ViewNode::IfBlock(block) => {
            out.push_str(&pad);
            out.push_str(&format!("if {}\n", block.condition));
            for child in &block.then_branch {
                emit_node(child, depth + 1, out);
            }
            if let Some(else_branch) = &block.else_branch {
                // An else-branch holding exactly one `if` is what `else if` parses to, so it re-emits as the chain: formatting a file must not rewrite the spelling its author chose.
                if let [ViewNode::IfBlock(chained)] = else_branch.as_slice() {
                    out.push_str(&pad);
                    out.push_str("else ");
                    let mut nested = String::new();
                    emit_node(&ViewNode::IfBlock(chained.clone()), depth, &mut nested);
                    out.push_str(nested.trim_start());
                } else {
                    out.push_str(&pad);
                    out.push_str("else\n");
                    for child in else_branch {
                        emit_node(child, depth + 1, out);
                    }
                }
            }
        }
        ViewNode::MatchBlock(block) => {
            out.push_str(&pad);
            out.push_str(&format!("match {}", block.scrutinee));
            if let Some(binding) = &block.binding {
                out.push_str(&format!(" as {binding}"));
            }
            if let Some(key) = block
                .key_expr
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                out.push_str(&format!(" key {key}"));
            }
            out.push('\n');
            for arm in &block.arms {
                out.push_str(&INDENT.repeat(depth + 1));
                out.push_str(&arm.pattern);
                out.push('\n');
                for child in &arm.body {
                    emit_node(child, depth + 2, out);
                }
            }
        }
        ViewNode::ForBlock(block) => {
            out.push_str(&pad);
            out.push_str(&format!("for {} in {}", block.pattern, block.iterable));
            // Both clauses are optional and must survive a round-trip: dropping `gap:N` silently changes a reactive row's spacing and, since the transpiler keys the transparent gap fragment off it, its whole layout path.
            if let Some(key) = block
                .key_expr
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                out.push_str(&format!(" key {key}"));
            }
            if let Some(gap) = block
                .gap_expr
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                out.push_str(&format!(" gap:{gap}"));
            }
            if let Some(height) = block
                .virtual_row_height
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                out.push_str(&format!(" virtual row_height:{height}"));
            }
            out.push('\n');
            for child in &block.body {
                emit_node(child, depth + 1, out);
            }
        }
        ViewNode::LetStmt(stmt) => {
            out.push_str(&pad);
            out.push_str(&stmt.source);
            out.push('\n');
        }
        ViewNode::Comment(text) => {
            out.push_str(&pad);
            out.push_str(text);
            out.push('\n');
        }
    }
}

/// Re-emits an element header as `tag .class "content" key:value`, the canonical token order (the parser accepts these in any order but splits them apart).
pub(super) fn format_element_header(element: &Element) -> String {
    let mut parts: Vec<String> = Vec::with_capacity(1 + element.classes.len());
    parts.push(element.tag.clone());
    for class in &element.classes {
        parts.push(format!("@{class}"));
    }
    if let Some(content) = &element.content {
        parts.push(format!("\"{}\"", escape_rsx_string(content)));
    }
    for attr in &element.attributes {
        parts.push(format_attr(attr));
    }
    parts.join(" ")
}

/// Escapes a string value for re-emission inside `"…"`. Parsed content is stored unescaped (the parser interprets `\n`/`\"`/… and raw `r"…"` keeps backslashes literal), so re-emitting verbatim would corrupt any value containing a quote, backslash, or control char on the next save. Always emits the escaped form; raw literals normalize to it (same content).
pub(super) fn escape_rsx_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\0' => out.push_str("\\0"),
            _ => out.push(c),
        }
    }
    out
}

/// Re-emits an attribute in the form it was written in. One arm per [`Value`] variant, because the variant is the whole of what decides the spelling: guessing it back from the text is what used to turn a `t"…"` key into a plain literal and a `transition(…)` into a colon the parser reads as one token.
pub(super) fn format_attr(attr: &Attr) -> String {
    match &attr.value {
        Value::Flag => attr.key.clone(),
        Value::Expr(text) => format!("{}:{text}", attr.key),
        Value::Quoted(text) => format!("{}:\"{}\"", attr.key, escape_rsx_string(text)),
        Value::Directive(text) => format!("{}({text})", attr.key),
    }
}

/// Re-emits a `[preview "Name" key:value flag …]` section: the header (name plus options) followed by its body, formatted like a `[view]` tree.
pub(super) fn format_preview_section(preview: &Preview) -> String {
    let mut header = format!("[preview \"{}\"", preview.name);
    for opt in &preview.options {
        if opt.value.is_empty() {
            header.push_str(&format!(" {}", opt.key));
        } else {
            header.push_str(&format!(" {}:{}", opt.key, opt.value));
        }
    }
    header.push(']');

    let mut body = String::new();
    for node in &preview.body {
        emit_node(node, 0, &mut body);
    }
    let body = body.trim_end();
    if body.is_empty() {
        header
    } else {
        format!("{header}\n{body}")
    }
}
