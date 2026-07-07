use crate::analysis::color::{hex_string, parse_hex, rgba};
use crate::analysis::util::{attribute_key_before_colon, word_at_cursor};
use crate::position::{Section, find_section_at};
use crate::project::ProjectInfo;
use lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind};
use rsx_parser::{RsxDocument, StyleValue};
use rsx_transpiler::{color_attr_keys, keyword_color_rgba};

pub fn hover_info(
    doc: &RsxDocument,
    source: &str,
    line: u32,
    character: u32,
    project: Option<&ProjectInfo>,
) -> Option<Hover> {
    if find_section_at(source, line) != Section::View {
        return None;
    }
    let line_text = source.lines().nth(line as usize)?;
    let (word_start, word) = word_at_cursor(line_text, character as usize);
    if word.is_empty() {
        return None;
    }

    let char_before = line_text[..word_start].chars().last();

    if char_before == Some(':') {
        if let Some(key) = attribute_key_before_colon(line_text, word_start)
            && color_attr_keys().contains(&key)
        {
            return hover_color(doc, word, project);
        }
        return None;
    }

    let prefix_before_word = line_text[..word_start].trim();
    if prefix_before_word.is_empty() {
        return hover_tag(word);
    }

    None
}

fn hover_color(doc: &RsxDocument, value: &str, project: Option<&ProjectInfo>) -> Option<Hover> {
    if let Some(constant) = doc.style.constants.iter().find(|c| c.name == value) {
        let text = match &constant.value {
            StyleValue::Hex(hex) => format!("■ #{hex} — {value}"),
            StyleValue::Raw(raw) => format!("{value}: {raw}"),
            StyleValue::Number(n) => format!("{value}: {n}"),
        };
        return Some(make_hover(text));
    }
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

fn hover_tag(tag: &str) -> Option<Hover> {
    let rust_type = rsx_transpiler::builtin_tags()
        .iter()
        .find(|(name, _)| *name == tag)
        .map(|(_, ctor)| *ctor)?;
    if rust_type == rsx_transpiler::TAG_REFERENCES_VARIABLE {
        return Some(make_hover(format!(
            "`{tag}` → references an in-scope variable"
        )));
    }
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
    use rsx_parser::parse;

    fn hover_text(src: &str, line: u32, character: u32) -> Option<String> {
        let doc = parse(src).unwrap();
        match hover_info(&doc, src, line, character, None)?.contents {
            HoverContents::Markup(markup) => Some(markup.value),
            _ => None,
        }
    }

    #[test]
    fn keyword_color_hovers_a_swatch() {
        // `fill:white` is a completion candidate and a swatch, so it must hover consistently too.
        let src = "[view]\nbox fill:white\n";
        let text = hover_text(src, 1, 11).expect("hover over the `white` keyword");
        assert_eq!(text, "■ #ffffff — white");
    }

    #[test]
    fn hex_literal_hovers_a_normalized_swatch() {
        let src = "[view]\nbox fill:#f0a\n";
        let text = hover_text(src, 1, 11).expect("hover over the hex literal");
        assert_eq!(text, "■ #ff00aa");
    }
}
