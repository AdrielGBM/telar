use crate::analysis::occurrences::declared_signals;
use crate::position::{Section, find_section_at};
use crate::project::ProjectInfo;
use lsp_types::{CompletionItem, CompletionItemKind};
use rsx_parser::RsxDocument;
use rsx_transpiler::{color_attr_keys, color_keywords, is_control_flow_keyword, tag_attr_keys};
use std::collections::HashSet;
use std::path::Path;

pub enum CompletionKind {
    ElementName,
    AttributeKey(String),
    ColorValue,
    StyleClass,
    SignalRef,
}

pub fn completion_context(source: &str, line: u32, character: u32) -> Option<CompletionKind> {
    if find_section_at(source, line) != Section::View {
        return None;
    }

    let line_text = source.lines().nth(line as usize).unwrap_or("");
    let prefix = &line_text[..character.min(line_text.len() as u32) as usize];

    // Inside a quoted string (text content / `{…}` interpolation): defer to the embedded analyzer for Rust completion.
    if in_quoted_string(prefix) {
        return None;
    }

    let trimmed = prefix.trim_start();
    if trimmed.is_empty() || !trimmed.contains(char::is_whitespace) {
        return Some(CompletionKind::ElementName);
    }

    let mut tokens = trimmed.splitn(2, char::is_whitespace);
    let tag = tokens.next().unwrap_or("").to_string();
    let rest = tokens.next().unwrap_or("");

    // Control-flow lines carry Rust expressions, and `:|` marks a closure attribute value that runs to end of line — neither is an element/attr position.
    if is_control_flow_keyword(&tag) || rest.contains(":|") {
        return None;
    }

    let current_token = rest.split(char::is_whitespace).next_back().unwrap_or("");

    if current_token.starts_with('@') {
        return Some(CompletionKind::StyleClass);
    }
    if current_token.starts_with('$') {
        return Some(CompletionKind::SignalRef);
    }

    if let Some(colon_pos) = current_token.find(':') {
        let key = &current_token[..colon_pos];
        if color_attr_keys().contains(&key) {
            return Some(CompletionKind::ColorValue);
        }
        return None;
    }

    Some(CompletionKind::AttributeKey(tag))
}

/// Whether `prefix` ends inside an open double-quoted string, honoring `\"` escapes.
fn in_quoted_string(prefix: &str) -> bool {
    let mut in_str = false;
    let mut escaped = false;
    for c in prefix.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        match c {
            '\\' if in_str => escaped = true,
            '"' => in_str = !in_str,
            _ => {}
        }
    }
    in_str
}

pub fn element_name_items(dir: Option<&Path>) -> Vec<CompletionItem> {
    let builtin_set: HashSet<&str> = rsx_transpiler::builtin_tags()
        .iter()
        .map(|(tag, _)| *tag)
        .collect();

    let mut items: Vec<CompletionItem> = builtin_set
        .iter()
        .map(|name| CompletionItem {
            label: name.to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            ..Default::default()
        })
        .collect();

    if let Some(dir) = dir {
        for path in rsx_transpiler::find_rsx_files(dir) {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str())
                && !builtin_set.contains(stem)
            {
                items.push(CompletionItem {
                    label: stem.to_string(),
                    kind: Some(CompletionItemKind::MODULE),
                    ..Default::default()
                });
            }
        }
    }

    items
}

fn attribute_items(keys: &[&str]) -> Vec<CompletionItem> {
    keys.iter()
        .map(|k| CompletionItem {
            label: k.to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            insert_text: Some(format!("{k}:")),
            ..Default::default()
        })
        .collect()
}

pub fn attribute_key_items(tag: &str) -> Vec<CompletionItem> {
    attribute_items(&tag_attr_keys(tag))
}

pub fn color_items(doc: &RsxDocument, project: Option<&ProjectInfo>) -> Vec<CompletionItem> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut items: Vec<CompletionItem> = Vec::new();
    let push = |label: String, items: &mut Vec<CompletionItem>, seen: &mut HashSet<String>| {
        if seen.insert(label.clone()) {
            items.push(CompletionItem {
                label,
                kind: Some(CompletionItemKind::COLOR),
                ..Default::default()
            });
        }
    };

    for constant in &doc.style.constants {
        push(constant.name.clone(), &mut items, &mut seen);
    }
    if let Some(proj) = project {
        for field in &proj.theme_fields {
            push(field.clone(), &mut items, &mut seen);
        }
    }
    // Keyword colors (`white`/`black`/`transparent`), offered alongside `[style]` constants and theme fields.
    for keyword in color_keywords() {
        push(keyword.to_string(), &mut items, &mut seen);
    }

    items
}

pub fn style_class_items(doc: &RsxDocument) -> Vec<CompletionItem> {
    doc.style
        .classes
        .iter()
        .map(|class| CompletionItem {
            label: class.name.clone(),
            kind: Some(CompletionItemKind::CLASS),
            insert_text: Some(class.name.clone()),
            ..Default::default()
        })
        .collect()
}

/// Signals/memos declared in `[logic]`, offered after a `$` in `[view]`. `insert_text` drops the `$` (the trigger char is already typed), so completing leaves a single `$name`.
pub fn signal_items(source: &str) -> Vec<CompletionItem> {
    declared_signals(source)
        .into_iter()
        .map(|name| CompletionItem {
            label: format!("${name}"),
            kind: Some(CompletionItemKind::VARIABLE),
            insert_text: Some(name),
            ..Default::default()
        })
        .collect()
}
