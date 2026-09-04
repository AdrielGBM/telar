//! The rewrites that run over a `[view]` body: the colon form, the catalog macro, and the clip shorthand.

use super::imports::leading_token;
use super::text::{closing_paren, replace_outside_strings, string_end, top_level_space};
/// The keys whose `key(…)` is a grammar of its own. Everything else is a value and takes the colon.
pub(super) const DIRECTIVES: &[&str] = &[
    "transition",
    "hover_style",
    "active_style",
    "disabled_style",
    "focus_style",
    "cols",
    "stroke_width",
    "drag_button",
];

/// `key(expr)` → `key:expr`, parenthesised only where the expression holds a top-level space — which is what the parens are for now, and they are ordinary Rust rather than punctuation the DSL invented.
pub(super) fn colonise(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    for line in body.split_inclusive('\n') {
        out.push_str(&colonise_line(line));
    }
    out
}

pub(super) fn colonise_line(line: &str) -> String {
    // A control-flow line is Rust, not an attribute list: `if shown($seen)` and a `[view]`-level `let` both hold calls, and a call is not a key however much the shape rhymes.
    if leading_token(line).is_some_and(is_control_flow) {
        return line.to_string();
    }
    let bytes = line.as_bytes();
    let mut out = String::with_capacity(line.len());
    let mut i = 0;
    while i < line.len() {
        if let Some(end) = string_end(bytes, i) {
            out.push_str(&line[i..end]);
            i = end;
            continue;
        }
        // A value already in colon form is one token: `on_press:(|| f())` holds a call, and a call is not an attribute however much it looks like one.
        if bytes[i] == b'(' && i > 0 && bytes[i - 1] == b':' {
            let end = closing_paren(bytes, i).map(|c| c + 1).unwrap_or(line.len());
            out.push_str(&line[i..end]);
            i = end;
            continue;
        }
        // One char, not one byte: a `.rsx` line holds prose, and an em dash outside a string literal is three of them.
        let step = line[i..].chars().next().unwrap().len_utf8();
        let Some((key, open)) = key_before_paren(line, i) else {
            out.push_str(&line[i..i + step]);
            i += step;
            continue;
        };
        let Some(close) = closing_paren(bytes, open) else {
            out.push_str(&line[i..i + step]);
            i += step;
            continue;
        };
        if DIRECTIVES.contains(&key) {
            out.push_str(&line[i..=close]);
            i = close + 1;
            continue;
        }
        let inner = &line[open + 1..close];
        // The key is already in `out` — the walk reaches its `(` one byte at a time.
        out.truncate(out.len() - key.len());
        out.push_str(key);
        out.push(':');
        // Parenthesised when the expression holds a top-level space, and when it opens with `::`: a leading path separator against the colon reads as `key::…`, which is a key nobody wrote.
        match top_level_space(inner) || inner.trim_start().starts_with("::") {
            true => out.push_str(&format!("({inner})")),
            false => out.push_str(inner),
        }
        i = close + 1;
    }
    out
}

/// The keywords that open a Rust line rather than an element. `match` and `else` carry no parenthesised value of their own, but a scrutinee or a guard on the same line does.
pub(super) fn is_control_flow(word: &str) -> bool {
    matches!(word, "if" | "else" | "for" | "let" | "match" | "while")
}

/// The key immediately before the `(` at or after `i`, when that `(` opens an attribute's value.
pub(super) fn key_before_paren(line: &str, i: usize) -> Option<(&str, usize)> {
    let bytes = line.as_bytes();
    if bytes[i] != b'(' {
        return None;
    }
    let start = line[..i]
        .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .map(|at| at + 1)
        .unwrap_or(0);
    let key = &line[start..i];
    let leads_ok = key
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c == '_');
    // A key is preceded by whitespace: `f(x)` inside a value is a call, not an attribute.
    let preceded_by_space = start == 0 || bytes[start - 1].is_ascii_whitespace();
    (!key.is_empty() && leads_ok && preceded_by_space).then_some((key, i))
}

/// `key:t"nav.title"` → `key:t!("nav.title")`. The *content* position keeps `t"…"`, because there the literal is the syntax rather than a value.
pub(super) fn i18n_macro(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut rest = body;
    while let Some(at) = rest.find(":t\"") {
        let Some(end) = string_end(rest.as_bytes(), at + 2) else {
            break;
        };
        out.push_str(&rest[..at + 1]);
        out.push_str("t!(");
        out.push_str(&rest[at + 2..end]);
        out.push(')');
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

/// `clip:x` → `clip:Clip::x()`. A clip is a shape now, not an axis from a closed set of three.
pub(super) fn clip_shapes(body: &str) -> String {
    replace_outside_strings(body, |chunk| {
        chunk
            .replace("clip:x", "clip:Clip::x()")
            .replace("clip:y", "clip:Clip::y()")
            .replace("clip:both", "clip")
    })
}
