//! The `[logic]` zone: statement-level Rust wrapped into a synthetic item so `rustfmt` will take it, then unwrapped again.

use super::INDENT;
use super::rustfmt::rustfmt_stdin;

/// Synthetic wrapper used to make the statement-level `[logic]` zone a valid Rust item for `rustfmt`.
pub(super) const WRAPPER_FN: &str = "__rsx_fmt_logic_wrapper";

pub(super) fn format_logic_section(logic: &str) -> String {
    format_rust_zone("[logic]", logic)
}

/// Formats a zone of statement-level Rust under its `header`: `[logic]`, or a preview's `[play]`.
pub(super) fn format_rust_zone(header: &str, source: &str) -> String {
    let body = run_rustfmt_on_logic(source).unwrap_or_else(|| source.trim_end().to_string());
    let body = body.trim_end();
    if body.is_empty() {
        header.to_string()
    } else {
        format!("{header}\n{body}")
    }
}

/// Reformats a zone of statement-level Rust with `rustfmt`. Returns `None` (so the caller keeps the source verbatim) when `rustfmt` is missing or rejects the input.
pub(super) fn run_rustfmt_on_logic(logic: &str) -> Option<String> {
    let logic = logic.trim_end();
    if logic.trim().is_empty() {
        return None;
    }

    let wrapped = format!("fn {WRAPPER_FN}() {{\n{logic}\n}}\n");
    let formatted = rustfmt_stdin(&wrapped).ok()?;
    unwrap_logic(&formatted)
}

/// Strips the synthetic wrapper function and one level of indentation that `rustfmt` added, and turns preview sentinel comments back into attributes.
pub(super) fn unwrap_logic(formatted: &str) -> Option<String> {
    let lines: Vec<&str> = formatted.lines().collect();
    let first = lines.first()?;
    if !first.trim_start().starts_with(&format!("fn {WRAPPER_FN}")) {
        return None;
    }
    // The wrapper's closing brace is the last non-blank line.
    let close = lines.iter().rposition(|l| l.trim() == "}")?;
    if close == 0 {
        return None;
    }

    // rustfmt exits 0 having only partly reformatted a body it could not fully parse: the inline prop-default sugar is not valid Rust, so a `Props` struct using it comes back with the wrapper's indent on the lines around it and none on its own fields. There is then no right amount to strip, so keep the source verbatim.
    let body = &lines[1..close];
    if !body
        .iter()
        .all(|line| line.trim().is_empty() || line.starts_with(INDENT))
    {
        return None;
    }
    let body: Vec<String> = body
        .iter()
        .map(|line| line.strip_prefix(INDENT).unwrap_or(line).to_string())
        .collect();

    Some(body.join("\n").trim_end().to_string())
}
