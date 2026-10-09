//! Completion: what may be written at the cursor, decided from the `.rsx` section it is in.

use crate::analysis::occurrences::declared_signals;
use crate::analysis::preview_header::{self, HeaderKind, Place, TokenKind};
use crate::position::{Section, find_section_at};
use crate::project::ProjectInfo;
use crate::text::utf16_to_byte;
use lsp_types::{
    Command, CompletionItem, CompletionItemKind, CompletionItemLabelDetails, Documentation,
    InsertTextFormat, MarkupContent, MarkupKind,
};
use std::collections::HashSet;
use std::path::Path;
use telar_parser::{Preview, RsxDocument, ViewNode};
use telar_project::naming::to_pascal_case;
use telar_transpiler::{color_attr_keys, color_keywords, is_builtin_tag, is_control_flow_keyword};

/// What may be written at the cursor: an element name, an attribute key, a colour, a class, a signal, the name after `$theme.`, or a preview header's option, its value, or an inline matrix's axis or value.
#[derive(Debug, PartialEq, Eq)]
pub enum CompletionKind {
    ElementName,
    AttributeKey(String),
    ColorValue,
    StyleClass,
    SignalRef,
    ThemeToken,
    PreviewOption(HeaderKind),
    PreviewOptionValue(String),
    MatrixAxis,
    MatrixAxisValue(String),
}

/// What kind of completion the cursor is in, decided from the `.rsx` section around it. A preview header is read from its own line, so one still being typed, with no closing `]` yet, is completed as one.
pub fn completion_context(source: &str, line: u32, character: u32) -> Option<CompletionKind> {
    let line_text = source.lines().nth(line as usize).unwrap_or("");
    let prefix = &line_text[..utf16_to_byte(line_text, character)];
    if let Some((kind, place)) = preview_header::place_at_end(prefix) {
        return match place {
            Place::OptionKey => Some(CompletionKind::PreviewOption(kind)),
            Place::OptionValue(key) => Some(CompletionKind::PreviewOptionValue(key)),
            Place::MatrixAxis => Some(CompletionKind::MatrixAxis),
            Place::MatrixAxisValue(axis) => Some(CompletionKind::MatrixAxisValue(axis)),
            Place::ArgName | Place::ArgValue | Place::Elsewhere => None,
        };
    }
    if find_section_at(source, line) != Section::View {
        return None;
    }

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

/// The built-in tags, the components the `[telar] prelude` crates export, and the `.rsx` components discoverable from `dir` — not a `*.previews.rsx` or a `mod.rsx`, which declare none. A name is offered once: a built-in shadows a component of the same name, and a prelude component's entry carries more than a bare `.rsx` stem.
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
        let declares_a_component = |path: &Path| {
            !telar_project::is_previews_file(path) && !telar_project::is_module_root(path)
        };
        for path in telar_project::find_rsx_files_in_tree(dir)
            .into_iter()
            .filter(|path| declares_a_component(path))
        {
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

/// The signals a `$` may read on 0-based `line`, offered after it. A preview builds in a fn of its own, out of reach of the component's `[logic]`, so in a preview's body these are the names its `args(…)` declares; anywhere else, the signals and memos `[logic]` declares. `insert_text` drops the `$` (the trigger char is already typed), so completing leaves a single `$name`.
pub fn signal_items(source: &str, doc: &RsxDocument, line: u32) -> Vec<CompletionItem> {
    let names = match preview_at(doc, line) {
        Some(preview) => preview.args.iter().map(|arg| arg.name.clone()).collect(),
        None => declared_signals(source),
    };
    names
        .into_iter()
        .map(|name| CompletionItem {
            label: format!("${name}"),
            kind: Some(CompletionItemKind::VARIABLE),
            insert_text: Some(name),
            ..Default::default()
        })
        .collect()
}

/// The preview whose header or body holds 0-based `line`.
fn preview_at(doc: &RsxDocument, line: u32) -> Option<&Preview> {
    doc.previews
        .iter()
        .rev()
        .find(|preview| preview.line <= line as usize + 1)
}

/// The options a preview header takes, and on a variant, `args(…)`.
pub fn preview_option_items(kind: HeaderKind) -> Vec<CompletionItem> {
    let mut items: Vec<CompletionItem> = preview_header::options()
        .map(|option| {
            let mut item = snippet_item(option.key, option.snippet, option.doc);
            if option.snippet.ends_with(':') {
                item.command = Some(trigger_suggest());
            }
            item
        })
        .collect();
    if kind == HeaderKind::Variant {
        items.push(snippet_item(
            "args(…)",
            "args(${1:name}:${2:default})",
            preview_header::ARGS_DECL_DOC,
        ));
    }
    items
}

/// The values an option takes that can be listed: a layout, a direction, `none` for `args:`, and the matrices a preview may name.
pub fn preview_option_value_items(key: &str, project: Option<&ProjectInfo>) -> Vec<CompletionItem> {
    match key {
        "layout" => word_items(&[
            ("padded", "The preview with room around it."),
            (
                "centered",
                "The preview at its own size, in the middle of the canvas.",
            ),
            ("fullscreen", "The preview filling the canvas."),
        ]),
        "dir" => direction_items(),
        "args" => word_items(&[(
            "none",
            "Keeps the root's literal attributes fixed rather than turning each into an arg.",
        )]),
        "matrix" => matrix_items(project),
        _ => Vec::new(),
    }
}

/// The matrices a preview may name: the package's `[telar.previews.matrices]`, the built-in `themes` unless the package names its own, and the axes written out.
fn matrix_items(project: Option<&ProjectInfo>) -> Vec<CompletionItem> {
    let previews = project.map(|project| &project.previews);
    let names: Vec<&String> = previews
        .and_then(|previews| previews.matrices.as_ref())
        .map(|matrices| matrices.keys().collect())
        .unwrap_or_default();
    let read = previews
        .and_then(|previews| previews.named_matrices().ok())
        .unwrap_or_default();
    let mut items: Vec<CompletionItem> = names
        .iter()
        .map(|name| {
            let axes = read.get(name.as_str()).map(|axes| {
                axes.iter()
                    .map(|axis| axis.name())
                    .collect::<Vec<_>>()
                    .join(" × ")
            });
            CompletionItem {
                label: name.to_string(),
                kind: Some(CompletionItemKind::ENUM_MEMBER),
                label_details: Some(CompletionItemLabelDetails {
                    detail: axes.map(|axes| format!(" {axes}")),
                    description: Some("telar.toml".to_string()),
                }),
                ..Default::default()
            }
        })
        .collect();
    if !names.iter().any(|name| name.as_str() == BUILT_IN_THEMES) {
        items.push(CompletionItem {
            label: BUILT_IN_THEMES.to_string(),
            kind: Some(CompletionItemKind::ENUM_MEMBER),
            label_details: Some(CompletionItemLabelDetails {
                detail: Some(" mode".to_string()),
                description: Some("built in".to_string()),
            }),
            documentation: Some(markdown(
                "Every registered mode, side by side. A matrix the package names `themes` takes its place.",
            )),
            ..Default::default()
        });
    }
    items.push(snippet_item(
        "(…)",
        "(${1:mode}:[${2:light dark}])",
        "The matrix's axes written out, the first outermost: `matrix:(mode:[light dark] size:[12 16])`.",
    ));
    items
}

/// The matrix every package can name without declaring it.
const BUILT_IN_THEMES: &str = "themes";

/// The axes an inline matrix may vary on the header at 0-based `line`: the environment's, then each arg the preview has — the names its `args(…)` declares and the attributes of the component call it renders.
pub fn matrix_axis_items(doc: &RsxDocument, line: u32, line_text: &str) -> Vec<CompletionItem> {
    let mut items: Vec<CompletionItem> = telar_project::MATRIX_GLOBAL_AXES
        .iter()
        .map(|axis| axis_item(axis, "environment"))
        .collect();
    let mut seen: HashSet<String> = telar_project::MATRIX_GLOBAL_AXES
        .iter()
        .map(|axis| axis.to_string())
        .collect();
    let declared = preview_header::header_tokens(line_text)
        .into_iter()
        .filter(|token| token.kind == TokenKind::ArgName)
        .map(|token| line_text[token.start..token.start + token.len].to_string());
    let rendered = doc
        .previews
        .iter()
        .find(|preview| preview.line == line as usize + 1)
        .and_then(|preview| root_component(&preview.body))
        .into_iter()
        .flat_map(|root| &root.attributes)
        .filter(|attr| {
            attr.key != "slot" && !attr.value.text().contains('$') && !attr.value.is_closure()
        })
        .map(|attr| attr.key.clone());
    for name in declared.chain(rendered) {
        if seen.insert(name.clone()) {
            items.push(axis_item(&name, "arg"));
        }
    }
    items
}

/// The component call a preview renders, when its body is that one call: the call whose attributes are the preview's implicit args.
fn root_component(body: &[ViewNode]) -> Option<&telar_parser::Element> {
    let mut nodes = body
        .iter()
        .filter(|node| !matches!(node, ViewNode::Comment(_)));
    match (nodes.next(), nodes.next()) {
        (Some(ViewNode::Element(element)), None) if !is_builtin_tag(&element.tag) => Some(element),
        _ => None,
    }
}

fn axis_item(name: &str, description: &str) -> CompletionItem {
    CompletionItem {
        label: name.to_string(),
        kind: Some(CompletionItemKind::PROPERTY),
        label_details: Some(CompletionItemLabelDetails {
            detail: None,
            description: Some(description.to_string()),
        }),
        insert_text: Some(format!("{name}:[$1]")),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        command: Some(trigger_suggest()),
        ..Default::default()
    }
}

/// The values of an inline matrix axis that can be listed: a direction, a control size, or a viewport the package names.
pub fn matrix_axis_value_items(axis: &str, project: Option<&ProjectInfo>) -> Vec<CompletionItem> {
    match axis {
        "dir" => direction_items(),
        "control_size" => word_items(&[
            ("mini", "The smallest control size."),
            ("small", "A size below the regular one."),
            ("regular", "The default control size."),
            ("large", "A size above the regular one."),
        ]),
        "viewport" => project
            .and_then(|project| project.previews.viewports.as_ref())
            .into_iter()
            .flatten()
            .map(|(name, size)| CompletionItem {
                label: name.clone(),
                kind: Some(CompletionItemKind::ENUM_MEMBER),
                label_details: Some(CompletionItemLabelDetails {
                    detail: Some(format!(" {}x{}", size.width, size.height)),
                    description: Some("telar.toml".to_string()),
                }),
                ..Default::default()
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn direction_items() -> Vec<CompletionItem> {
    word_items(&[
        ("ltr", "Left to right."),
        ("rtl", "Right to left, whatever the locale's own direction."),
    ])
}

fn word_items(words: &[(&str, &str)]) -> Vec<CompletionItem> {
    words
        .iter()
        .map(|(word, doc)| CompletionItem {
            label: word.to_string(),
            kind: Some(CompletionItemKind::ENUM_MEMBER),
            documentation: Some(markdown(doc)),
            ..Default::default()
        })
        .collect()
}

fn snippet_item(label: &str, snippet: &str, doc: &str) -> CompletionItem {
    CompletionItem {
        label: label.to_string(),
        kind: Some(CompletionItemKind::PROPERTY),
        documentation: Some(markdown(doc)),
        insert_text: Some(snippet.to_string()),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        ..Default::default()
    }
}

fn markdown(text: &str) -> Documentation {
    Documentation::MarkupContent(MarkupContent {
        kind: MarkupKind::Markdown,
        value: text.to_string(),
    })
}

/// Opens the list again once a key and its colon are in, so the values follow without another keystroke.
fn trigger_suggest() -> Command {
    Command {
        title: "Suggest".to_string(),
        command: "editor.action.triggerSuggest".to_string(),
        arguments: None,
    }
}

#[cfg(test)]
#[path = "completions_test.rs"]
mod tests;
