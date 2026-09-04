//! Hover: what a tag, a colour or a class says about itself.

use crate::analysis::color::{hex_string, parse_hex, rgba};
use crate::analysis::util::{ViewToken, view_token_at};
use crate::project::ProjectInfo;
use lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind};
use telar_transpiler::{ValueKind, keyword_color_rgba};

/// What the token under the cursor says about itself, for tags, colours and classes.
pub fn hover_info(
    source: &str,
    line: u32,
    character: u32,
    project: Option<&ProjectInfo>,
) -> Option<Hover> {
    match view_token_at(source, line, character)? {
        ViewToken::ColorValue(value) => hover_color(value, project),
        ViewToken::Attr { tag, key } => hover_attr(tag, key),
        ViewToken::Tag(tag) => hover_tag(tag),
        // A style class already shows its own definition through goto-definition; there is no tooltip for it.
        ViewToken::Class(_) => None,
    }
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
    Some(make_hover(format!("`{tag}` → `{rust_type}()`")))
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
mod tests {
    use super::*;

    fn hover_text(src: &str, line: u32, character: u32) -> Option<String> {
        match hover_info(src, line, character, None)?.contents {
            HoverContents::Markup(markup) => Some(markup.value),
            _ => None,
        }
    }

    #[test]
    fn keyword_color_hovers_a_swatch() {
        let src = "[view]\nbox fill:transparent\n";
        let text = hover_text(src, 1, 11).expect("hover over the `transparent` keyword");
        assert_eq!(text, "■ #00000000 — transparent");
    }

    #[test]
    fn hex_literal_hovers_a_normalized_swatch() {
        let src = "[view]\nbox fill:#f0a\n";
        let text = hover_text(src, 1, 11).expect("hover over the hex literal");
        assert_eq!(text, "■ #ff00aa");
    }
}
