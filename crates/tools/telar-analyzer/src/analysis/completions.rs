//! Completion: what may be written at the cursor, decided from the `.rsx` section it is in.

use crate::analysis::occurrences::declared_signals;
use crate::position::{Section, find_section_at};
use crate::project::ProjectInfo;
use lsp_types::{CompletionItem, CompletionItemKind, CompletionItemLabelDetails, Documentation};
use std::collections::HashSet;
use std::path::Path;
use telar_parser::RsxDocument;
use telar_project::naming::to_pascal_case;
use telar_transpiler::{color_attr_keys, color_keywords, is_control_flow_keyword};

/// What may be written at the cursor: an element name, an attribute key, a colour, a class, a signal or the name after `$theme.`.
#[derive(Debug, PartialEq, Eq)]
pub enum CompletionKind {
    ElementName,
    AttributeKey(String),
    ColorValue,
    StyleClass,
    SignalRef,
    ThemeToken,
}

/// What kind of completion the cursor is in, decided from the `.rsx` section around it.
pub fn completion_context(source: &str, line: u32, character: u32) -> Option<CompletionKind> {
    if find_section_at(source, line) != Section::View {
        return None;
    }

    let line_text = source.lines().nth(line as usize).unwrap_or("");
    let prefix = &line_text[..character.min(line_text.len() as u32) as usize];

    let string = string_state(prefix);
    if string != StringState::Text && ends_in_theme_read(prefix) {
        return Some(CompletionKind::ThemeToken);
    }
    // Inside a quoted string (text content / `{…}` interpolation): defer to the embedded analyzer for Rust completion.
    if string != StringState::Outside {
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

/// Where the end of a line prefix sits relative to its double-quoted strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StringState {
    Outside,
    /// In a string's own text, where `$theme` is just characters.
    Text,
    /// In a `{…}` interpolation inside a string, which is markup again.
    Interpolation,
}

/// Where `prefix` ends: outside every string, in a string's text, or in one of its `{…}` interpolations. Honors `\"` escapes.
fn string_state(prefix: &str) -> StringState {
    let mut in_str = false;
    let mut depth = 0u32;
    let mut escaped = false;
    for c in prefix.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        match c {
            '\\' if in_str => escaped = true,
            '"' => {
                in_str = !in_str;
                depth = 0;
            }
            '{' if in_str => depth += 1,
            '}' if in_str => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    match (in_str, depth) {
        (false, _) => StringState::Outside,
        (true, 0) => StringState::Text,
        (true, _) => StringState::Interpolation,
    }
}

/// Whether `prefix` ends in `$theme.` followed by the part of a name typed so far.
fn ends_in_theme_read(prefix: &str) -> bool {
    let is_ident = |c: char| c.is_ascii_alphanumeric() || c == '_';
    prefix
        .trim_end_matches(is_ident)
        .strip_suffix("$theme.")
        .is_some_and(|before| !before.ends_with(is_ident))
}

/// The built-in tags, the components the `[telar] prelude` crates export, and the `.rsx` components discoverable from `dir`. A name is offered once: a built-in shadows a component of the same name, and a prelude component's entry carries more than a bare `.rsx` stem.
pub fn element_name_items(dir: Option<&Path>, prelude: &[PreludeComponent]) -> Vec<CompletionItem> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut items: Vec<CompletionItem> = Vec::new();

    for (tag, _) in telar_transpiler::builtin_tags() {
        if seen.insert(tag.to_string()) {
            items.push(CompletionItem {
                label: tag.to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                ..Default::default()
            });
        }
    }

    for component in prelude {
        if seen.insert(component.name.clone()) {
            items.push(component.completion_item());
        }
    }

    if let Some(dir) = dir {
        for path in telar_project::find_rsx_files_in_tree(dir) {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str())
                && seen.insert(stem.to_string())
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

/// A component a `[telar] prelude` entry exports: a function `name` beside a `NameProps` type, which is what a tag compiles to a call of.
#[derive(Debug, Clone, PartialEq)]
pub struct PreludeComponent {
    pub name: String,
    /// The prelude entry it came through, shown beside the name so a tag's origin is visible in the list.
    pub module: String,
    pub detail: Option<String>,
    pub documentation: Option<Documentation>,
}

impl PreludeComponent {
    /// The component a [`component_items`] entry describes, exported through `module`.
    pub fn from_item(module: &str, item: CompletionItem) -> Self {
        Self {
            name: item_name(&item.label).to_string(),
            module: module.to_string(),
            detail: item.detail,
            documentation: item.documentation,
        }
    }

    fn completion_item(&self) -> CompletionItem {
        CompletionItem {
            label: self.name.clone(),
            kind: Some(CompletionItemKind::MODULE),
            label_details: Some(CompletionItemLabelDetails {
                detail: None,
                description: Some(self.module.clone()),
            }),
            detail: self.detail.clone(),
            documentation: self.documentation.clone(),
            ..Default::default()
        }
    }
}

/// The components among `items`, rust-analyzer's completions for `<module>::`: every function whose `NameProps` type is exported beside it. Each comes back as rust-analyzer gave it, so its detail and documentation can still be resolved before [`PreludeComponent::from_item`] reads it.
pub fn component_items(items: &[CompletionItem]) -> Vec<CompletionItem> {
    let types: HashSet<&str> = items
        .iter()
        .filter(|item| item.kind == Some(CompletionItemKind::STRUCT))
        .map(|item| item_name(&item.label))
        .collect();
    let mut seen: HashSet<&str> = HashSet::new();
    items
        .iter()
        .filter(|item| item.kind == Some(CompletionItemKind::FUNCTION))
        .filter(|item| {
            let name = item_name(&item.label);
            !name.is_empty()
                && types.contains(format!("{}Props", to_pascal_case(name)).as_str())
                && seen.insert(name)
        })
        .cloned()
        .collect()
}

/// The identifier a completion label starts with. rust-analyzer decorates the label for a client that cannot show label details — `button(…)`, `ButtonProps {…}` — so the name is only its leading identifier.
fn item_name(label: &str) -> &str {
    let end = label
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(label.len());
    &label[..end]
}

fn attribute_items(specs: &[telar_transpiler::AttrSpec]) -> Vec<CompletionItem> {
    specs
        .iter()
        .map(|spec| CompletionItem {
            label: spec.key.to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            insert_text: Some(format!("{}:", spec.key)),
            documentation: spec
                .doc
                .map(|doc| lsp_types::Documentation::String(doc.to_string())),
            ..Default::default()
        })
        .collect()
}

/// The attribute keys a tag accepts.
pub fn attribute_key_items(tag: &str) -> Vec<CompletionItem> {
    attribute_items(&telar_transpiler::tag_attr_specs(tag))
}

/// The colour keywords, plus the project theme's own tokens.
pub fn color_items(project: Option<&ProjectInfo>) -> Vec<CompletionItem> {
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

    if let Some(proj) = project {
        for field in &proj.theme_fields {
            push(field.clone(), &mut items, &mut seen);
        }
    }
    for keyword in color_keywords() {
        push(keyword.to_string(), &mut items, &mut seen);
    }

    items
}

/// The names `$theme.` reads: the application theme type's fields, or, in a `[telar] library` that cannot name that type, the `ThemeTokens` every theme answers.
pub fn theme_items(project: Option<&ProjectInfo>) -> Vec<CompletionItem> {
    let Some(project) = project else {
        return Vec::new();
    };
    if project.library {
        return telar_project::theme_tokens::all()
            .map(|token| CompletionItem {
                label: token.to_string(),
                kind: Some(CompletionItemKind::PROPERTY),
                detail: Some("ThemeTokens".to_string()),
                ..Default::default()
            })
            .collect();
    }
    let mut fields: Vec<&String> = project.theme_fields.iter().collect();
    fields.sort();
    fields
        .into_iter()
        .map(|field| CompletionItem {
            label: field.clone(),
            kind: Some(CompletionItemKind::FIELD),
            detail: project.theme_type.clone(),
            ..Default::default()
        })
        .collect()
}

/// The `[style]` classes this document declares.
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

#[cfg(test)]
#[path = "completions_test.rs"]
mod tests;
