//! The `use` lines a file needs for the component tags it names, now that the crate root no longer supplies them.

use std::collections::BTreeMap;
use std::path::PathBuf;

use super::view::is_control_flow;
use super::zones::{Section, zones};
/// `stem -> crate::path::to::stem` for every `.rsx` in the sweep, so a tag that used to resolve through the crate-root re-export can be given the `use` line it now needs.
pub(super) fn component_modules(sources: &[PathBuf]) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for path in sources {
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Some(src_at) = path.components().position(|c| c.as_os_str() == "src") else {
            continue;
        };
        let segments: Vec<String> = path
            .with_extension("")
            .components()
            .skip(src_at + 1)
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .collect();
        out.insert(stem.to_string(), format!("crate::{}", segments.join("::")));
    }
    out
}

/// Adds a `use` line to `[logic]` for every component tag the file calls and does not already import. Each `.rsx` is a module now, so a caller imports what it uses instead of reaching a crate-root re-export.
pub(super) fn imports_for_tags(
    source: &str,
    modules: &BTreeMap<String, String>,
    own: &str,
) -> String {
    let mut wanted: Vec<&str> = Vec::new();
    for zone in zones(source) {
        if !matches!(zone.section, Section::View | Section::Preview) {
            continue;
        }
        for line in zone.body.lines() {
            let Some(tag) = leading_tag(line) else {
                continue;
            };
            if tag != own && modules.contains_key(tag) && !wanted.contains(&tag) {
                wanted.push(tag);
            }
        }
    }
    let missing: Vec<&&str> = wanted
        .iter()
        .filter(|tag| !source.contains(&format!("::{tag}::")))
        .collect();
    if missing.is_empty() {
        return source.to_string();
    }

    let lines: Vec<String> = missing
        .iter()
        .map(|tag| {
            let path = &modules[**tag];
            format!("use {path}::{{{tag}, {}Props}};\n", pascal(tag))
        })
        .collect();

    let mut out = String::with_capacity(source.len());
    let mut placed = false;
    for zone in zones(source) {
        out.push_str(zone.header);
        if zone.section == Section::Logic && !placed {
            placed = true;
            for line in &lines {
                out.push_str(line);
            }
        }
        out.push_str(zone.body);
    }
    match placed {
        true => out,
        // A file with no `[logic]` at all gets one, since a `use` has nowhere else to live.
        false => format!("[logic]\n{}\n{out}", lines.concat()),
    }
}

pub(super) fn leading_tag(line: &str) -> Option<&str> {
    let tag = leading_token(line)?;
    let ok = tag
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c == '_')
        && tag.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    (ok && !is_control_flow(tag)).then_some(tag)
}

/// The first word of a line, skipping its indent. `None` for a blank line or a comment.
pub(super) fn leading_token(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//") {
        return None;
    }
    let token = trimmed.split([' ', '\t', '(', ':']).next()?;
    (!token.is_empty()).then_some(token)
}

pub(super) fn pascal(name: &str) -> String {
    name.split('_')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(c) => c.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}
