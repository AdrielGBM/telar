use crate::analysis::util::{attr_key_before_colon, word_at_cursor};
use crate::position::{Section, find_section_at};
use crate::project::ProjectInfo;
use rsx_parser::{RsxDocument, StyleValue};
use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind};

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
        if let Some(key) = attr_key_before_colon(line_text, word_start) {
            if matches!(key, "color" | "fill" | "stroke" | "outline") {
                return hover_color(doc, word, project);
            }
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
    if let Some(proj) = project {
        if proj.theme_fields.contains(value) {
            let type_name = proj.theme_type.as_deref().unwrap_or("Theme");
            return Some(make_hover(format!("{type_name}.{value}")));
        }
    }
    None
}

fn hover_tag(tag: &str) -> Option<Hover> {
    let rust_type = match tag {
        "text" => "Text::new",
        "btn" | "button" => "Button::new",
        "col" | "column" | "row" | "grid" => "Container::new",
        "canvas" => "Canvas::new",
        "img" => "Image::new",
        "scroll" => "Scroll::new",
        "box" => "Box::new",
        _ => return None,
    };
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
