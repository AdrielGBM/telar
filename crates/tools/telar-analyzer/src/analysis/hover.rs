//! Hover: what a tag, a colour, a class or a preview header's option says about itself.

use crate::analysis::color::{hex_string, parse_hex, rgba};
use crate::analysis::preview_header::{self, TokenKind};
use crate::analysis::util::{ViewToken, view_token_at};
use crate::project::ProjectInfo;
use crate::text::utf16_to_byte;
use lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind};
use telar_transpiler::{ValueKind, keyword_color_rgba};

/// What the token under the cursor says about itself, for tags, colours, classes and preview header options.
pub fn hover_info(
    source: &str,
    line: u32,
    character: u32,
    project: Option<&ProjectInfo>,
) -> Option<Hover> {
    if let Some(hover) = hover_preview_option(source, line, character) {
        return Some(hover);
    }
    match view_token_at(source, line, character)? {
        ViewToken::ColorValue(value) => hover_color(value, project),
        ViewToken::Attr { tag, key } => hover_attr(tag, key),
        ViewToken::Tag(tag) => hover_tag(tag),
        // A style class already shows its own definition through goto-definition; there is no tooltip for it.
        ViewToken::Class(_) => None,
    }
}

/// What a preview header's option key, or its `args`, does.
fn hover_preview_option(source: &str, line: u32, character: u32) -> Option<Hover> {
    let line_text = source.lines().nth(line as usize)?;
    let (token, key) = preview_header::token_at(line_text, utf16_to_byte(line_text, character))?;
    if token.kind != TokenKind::OptionKey {
        return None;
    }
    let declares = line_text[token.start + token.len..].starts_with('(');
    let doc = match declares {
        true => preview_header::ARGS_DECL_DOC,
        false => preview_header::option(key)?.doc,
    };
    Some(make_hover(format!("`{key}` — preview option\n\n{doc}")))
}

fn hover_color(value: &str, project: Option<&ProjectInfo>) -> Option<Hover> {
    if let Some(proj) = project
        && proj.theme_fields.contains(value)
    {
        let type_name = proj.theme_type.as_deref().unwrap_or("Theme");
        return Some(make_hover(format!("{type_name}.{value}")));
    }
    // Keyword colors and raw hex literals: a swatch, matching the completion/`documentColor` surfaces.
    if let Some([r, g, b, a]) = keyword_color_rgba(value) {
        return Some(make_hover(format!(
            "■ {} — {value}",
            hex_string(rgba(r, g, b, a))
        )));
    }
    if let Some(color) = parse_hex(value) {
        return Some(make_hover(format!("■ {}", hex_string(color))));
    }
    None
}

/// What an attribute takes and what it does, read off the same table the emitter validates against.
fn hover_attr(tag: &str, key: &str) -> Option<Hover> {
    let spec = telar_transpiler::attr_spec(tag, key)?;
    let mut text = format!("`{key}`");
    if let Some(takes) = value_kind_label(spec.kind) {
        text.push_str(&format!(" — {takes}"));
    }
    if let Some(doc) = spec.doc {
        text.push_str(&format!("\n\n{doc}"));
    }
    Some(make_hover(text))
}

/// The values a key takes, spelled the way an author would write them. `None` for a key only rustc can judge.
fn value_kind_label(kind: Option<ValueKind>) -> Option<String> {
    let spellings = |table: &'static [(&'static str, &'static str)]| {
        table
            .iter()
            .map(|(name, _)| *name)
            .filter(|name| !name.is_empty())
            .collect::<Vec<_>>()
            .join(" | ")
    };
    Some(match kind? {
        ValueKind::Keywords(table) => spellings(table),
        ValueKind::KeywordsOrNumber(table) => format!("{} | a number", spellings(table)),
        ValueKind::Number => "a number".to_string(),
        ValueKind::Boolean => "true | false".to_string(),
        ValueKind::Edges => "one number, or one per edge".to_string(),
        ValueKind::Color => "a colour".to_string(),
    })
}

fn hover_tag(tag: &str) -> Option<Hover> {
    let rust_type = telar_transpiler::builtin_tags()
        .iter()
        .find(|(name, _)| *name == tag)
        .map(|(_, ctor)| *ctor)?;
    let signature = format!("`{tag}` → `{rust_type}()`");
    Some(make_hover(match telar_transpiler::builtin_tag_doc(tag) {
        Some(doc) => format!("{signature}\n\n{doc}"),
        None => signature,
    }))
}

fn make_hover(text: String) -> Hover {
    Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: text,
        }),
        range: None,
    }
}

#[cfg(test)]
#[path = "hover_test.rs"]
mod tests;
