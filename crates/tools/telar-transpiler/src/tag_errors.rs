//! What a compiler error in generated code means for the markup, when it lands on Rust the transpiler wrote for a tag or for a glob import.
//!
//! An unknown tag reaches rustc as an unresolved function and an unresolved `<Name>Props`, and a tag two crates export as an ambiguous glob import. Both read as complaints about Rust the author never wrote. `cargo telar check`, the `cargo telar dev` loop and the editor all rewrite them through here, so the three cannot word one tag error differently.
//!
//! The recogniser reads the generated line back in the shape [`crate::view`] emits it, through the same [`props_type`] and [`glob_import`], so the two cannot drift apart without a test noticing.

use telar_project::PreludeEntry;
use telar_project::naming::to_pascal_case;

use crate::view::props_type;

/// The line, without its newline, that brings `path`'s items into every generated component file.
pub(crate) fn glob_import(path: &str) -> String {
    format!("#[allow(unused_imports)] use {path}::*;")
}

/// What the generated Rust under a span was written for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GeneratedSite<'a> {
    /// A component call. `tag` is spelled as the author wrote it; `name` is the identifier under the span — the tag's own last segment, its `Props` type, or a module segment of a path tag.
    Tag { tag: &'a str, name: &'a str },
    /// One of the glob imports every component file opens with: `telar`, a `[telar] prelude` entry, or `crate`.
    Glob(&'a str),
}

/// What the generated line holding `generated[start..end]` was written for, or `None` when it is neither a component call nor a glob import, or the span misses the call's tag and `Props` type.
pub fn generated_site(generated: &str, start: usize, end: usize) -> Option<GeneratedSite<'_>> {
    if start > end || !generated.is_char_boundary(start) || !generated.is_char_boundary(end) {
        return None;
    }
    let line_start = generated[..start].rfind('\n').map_or(0, |at| at + 1);
    let line_end = generated[start..]
        .find('\n')
        .map_or(generated.len(), |at| start + at);
    let line = &generated[line_start..line_end];
    if let Some(path) = glob_path(line.trim()) {
        return Some(GeneratedSite::Glob(path));
    }

    let body = line.trim_start();
    let body_at = line_start + (line.len() - body.len());
    let (call, call_at) = match body.strip_prefix("let ") {
        Some(binding) => {
            let (_, call) = binding.split_once(" = ")?;
            (call, body_at + (body.len() - call.len()))
        }
        None => (body, body_at),
    };
    let (tag, after_tag) = call.split_once('(')?;
    if !is_path(tag) {
        return None;
    }
    let props = props_type(tag);
    if !after_tag.starts_with(&format!("{props}::props()")) {
        return None;
    }
    let props_at = call_at + tag.len() + 1;
    let within = |at: usize, len: usize| start >= at && end <= at + len;
    if !within(call_at, tag.len()) && !within(props_at, props.len()) {
        return None;
    }
    Some(GeneratedSite::Tag {
        tag,
        name: identifier_at(generated, start)?,
    })
}

/// A compiler error `code` at `site`, said about the markup, or `None` when it is not one a tag explains.
///
/// E0425 (the call) and E0433 (its `Props` path, or a path tag's module) are how an unresolved tag arrives. E0412 never does: the call names `Props` in expression position only. E0659 is a tag two glob imports provide, and `candidates` are the paths of those imports in rustc's order; with fewer than two there is nothing to name and rustc's own wording stands.
pub fn tag_error_message(
    code: &str,
    site: &GeneratedSite<'_>,
    prelude: &[PreludeEntry],
    candidates: &[String],
) -> Option<String> {
    let GeneratedSite::Tag { tag, name } = site else {
        return None;
    };
    match code {
        "E0425" | "E0433" => Some(unknown_tag(tag, prelude)),
        "E0659" => match candidates {
            [first, second, ..] => Some(clashing_tag(tag, name, first, second)),
            _ => None,
        },
        _ => None,
    }
}

/// The byte columns `tag` covers on the `.rsx` line that opens with it, so a rewritten error can underline the tag itself rather than the Rust it became.
pub fn tag_columns(rsx_line: &str, tag: &str) -> Option<(usize, usize)> {
    let body = rsx_line.trim_start();
    let rest = body.strip_prefix(tag)?;
    if rest
        .chars()
        .next()
        .is_some_and(|c| c == ':' || is_identifier_char(c))
    {
        return None;
    }
    let at = rsx_line.len() - body.len();
    Some((at, at + tag.len()))
}

fn unknown_tag(tag: &str, prelude: &[PreludeEntry]) -> String {
    let entries = match prelude {
        [] => "this package declares none".to_string(),
        entries => entries
            .iter()
            .map(|entry| format!("`{entry}`"))
            .collect::<Vec<_>>()
            .join(", "),
    };
    format!(
        "unknown tag `{tag}`: not a built-in, not a `.rsx` in this package, and not exported by any `[telar] prelude` entry ({entries})"
    )
}

/// The `use` that settles the clash. A bare tag is two names, the function and its `Props`, and both are ambiguous, so importing only the one rustc stopped at first would leave the other failing. A path tag clashes on its module, which one name settles.
fn clashing_tag(tag: &str, name: &str, first: &str, second: &str) -> String {
    let pick = match tag.contains("::") {
        true => name.to_string(),
        false => format!("{{{tag}, {}Props}}", to_pascal_case(tag)),
    };
    format!(
        "tag `{tag}` is exported by both `{first}` and `{second}`; pick one with `use {first}::{pick};` in `[logic]`"
    )
}

fn glob_path(line: &str) -> Option<&str> {
    let path = line
        .strip_prefix("#[allow(unused_imports)] use ")?
        .strip_suffix("::*;")?;
    (is_path(path) && glob_import(path) == line).then_some(path)
}

fn is_path(text: &str) -> bool {
    !text.is_empty() && text.split("::").all(is_identifier)
}

fn is_identifier(segment: &str) -> bool {
    segment
        .chars()
        .next()
        .is_some_and(|first| first == '_' || first.is_alphabetic())
        && segment.chars().all(is_identifier_char)
}

fn is_identifier_char(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

/// The identifier `text[at]` falls inside.
fn identifier_at(text: &str, at: usize) -> Option<&str> {
    let start = text[..at]
        .char_indices()
        .rev()
        .take_while(|(_, c)| is_identifier_char(*c))
        .last()
        .map_or(at, |(index, _)| index);
    let end = text[at..]
        .char_indices()
        .find(|(_, c)| !is_identifier_char(*c))
        .map_or(text.len(), |(index, _)| at + index);
    (start < end).then(|| &text[start..end])
}

#[cfg(test)]
#[path = "tag_errors_test.rs"]
mod tests;
