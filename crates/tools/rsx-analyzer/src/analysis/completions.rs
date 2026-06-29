use crate::position::{Section, find_section_at};
use crate::project::ProjectInfo;
use lsp_types::{CompletionItem, CompletionItemKind};
use rsx_parser::RsxDocument;
use std::collections::HashSet;
use std::path::Path;

pub enum CompletionKind {
    ElementName,
    AttributeKey(String),
    ColorValue,
    StyleClass,
}

pub fn completion_context(source: &str, line: u32, character: u32) -> Option<CompletionKind> {
    if find_section_at(source, line) != Section::View {
        return None;
    }

    let line_text = source.lines().nth(line as usize).unwrap_or("");
    let prefix = &line_text[..character.min(line_text.len() as u32) as usize];

    // Inside a quoted string (text content, including `{…}` interpolation): not an element/attr
    // position. Returning None lets the embedded analyzer answer Rust completion in interpolations.
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

    // Control-flow lines carry Rust expressions, and `:|` marks a closure attribute value that runs
    // to end of line — neither is an element/attr position, so defer to the embedded analyzer.
    if matches!(tag.as_str(), "if" | "for" | "let" | "else") || rest.contains(":|") {
        return None;
    }

    let current_token = rest.split(char::is_whitespace).next_back().unwrap_or("");

    if current_token.starts_with('.') {
        return Some(CompletionKind::StyleClass);
    }

    if let Some(colon_pos) = current_token.find(':') {
        let key = &current_token[..colon_pos];
        if matches!(key, "color" | "fill" | "stroke" | "outline") {
            return Some(CompletionKind::ColorValue);
        }
        return None;
    }

    Some(CompletionKind::AttributeKey(tag))
}

/// Whether `prefix` ends inside an open double-quoted string, honoring `\"` escapes. Used to suppress
/// element/attribute completion within `.rsx` string content.
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

    let mut items: Vec<CompletionItem> = rsx_transpiler::builtin_tags()
        .iter()
        .map(|(name, _)| CompletionItem {
            label: name.to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            ..Default::default()
        })
        .collect();

    if let Some(dir) = dir {
        for path in rsx_transpiler::find_rsx_files(dir) {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                if !builtin_set.contains(stem) {
                    items.push(CompletionItem {
                        label: stem.to_string(),
                        kind: Some(CompletionItemKind::MODULE),
                        ..Default::default()
                    });
                }
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
    let layout = rsx_transpiler::layout_attr_keys();

    match tag {
        "text" => attribute_items(&["size", "color"]),
        "widget" => vec![],
        "btn" | "button" => {
            let mut keys: Vec<&str> = layout.to_vec();
            keys.extend_from_slice(&["on_press", "fill", "outline"]);
            attribute_items(&keys)
        }
        "grid" => {
            let mut keys: Vec<&str> = layout.to_vec();
            keys.extend_from_slice(&["cols", "span", "row_span"]);
            attribute_items(&keys)
        }
        "box" => {
            let mut keys: Vec<&str> = layout.to_vec();
            keys.extend_from_slice(&[
                "fill",
                "stroke",
                "radius",
                "shadow_x",
                "shadow_y",
                "shadow_blur",
                "shadow_color",
            ]);
            attribute_items(&keys)
        }
        "img" => {
            let mut keys: Vec<&str> = layout.to_vec();
            keys.push("src");
            attribute_items(&keys)
        }
        _ => attribute_items(layout),
    }
}

pub fn color_items(doc: &RsxDocument, project: Option<&ProjectInfo>) -> Vec<CompletionItem> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut items: Vec<CompletionItem> = Vec::new();

    for constant in &doc.style.constants {
        if seen.insert(constant.name.clone()) {
            items.push(CompletionItem {
                label: constant.name.clone(),
                kind: Some(CompletionItemKind::COLOR),
                ..Default::default()
            });
        }
    }

    if let Some(proj) = project {
        for field in &proj.theme_fields {
            if seen.insert(field.clone()) {
                items.push(CompletionItem {
                    label: field.clone(),
                    kind: Some(CompletionItemKind::COLOR),
                    ..Default::default()
                });
            }
        }
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
