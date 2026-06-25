use crate::analysis::util::{attribute_key_before_colon, word_at_cursor};
use crate::position::{Section, find_section_at, parser_line_to_lsp_range};
use crate::project::ProjectInfo;
use rsx_parser::RsxDocument;
use tower_lsp::lsp_types::{GotoDefinitionResponse, Location, Url};

fn is_builtin_tag(tag: &str) -> bool {
    rsx_transpiler::builtin_tags()
        .iter()
        .any(|(name, _)| *name == tag)
}

pub fn goto_definition(
    doc: &RsxDocument,
    source: &str,
    uri: &Url,
    line: u32,
    character: u32,
    project: Option<&ProjectInfo>,
) -> Option<GotoDefinitionResponse> {
    if find_section_at(source, line) != Section::View {
        return None;
    }
    let line_text = source.lines().nth(line as usize)?;
    let (word_start, word) = word_at_cursor(line_text, character as usize);
    if word.is_empty() {
        return None;
    }

    let char_before = line_text[..word_start].chars().last();

    if char_before == Some('.') {
        return find_class(doc, word, uri);
    }

    if char_before == Some(':') {
        if let Some(key) = attribute_key_before_colon(line_text, word_start) {
            if matches!(key, "color" | "fill" | "stroke" | "outline") {
                return find_color(doc, word, uri, project);
            }
        }
        return None;
    }

    let prefix_before_word = line_text[..word_start].trim();
    if prefix_before_word.is_empty() && !is_builtin_tag(word) {
        return find_component(word, project, uri);
    }

    None
}

fn find_class(doc: &RsxDocument, name: &str, uri: &Url) -> Option<GotoDefinitionResponse> {
    let class = doc.style.classes.iter().find(|c| c.name == name)?;
    Some(GotoDefinitionResponse::Scalar(Location {
        uri: uri.clone(),
        range: parser_line_to_lsp_range(class.line),
    }))
}

fn find_color(
    doc: &RsxDocument,
    value: &str,
    uri: &Url,
    project: Option<&ProjectInfo>,
) -> Option<GotoDefinitionResponse> {
    if let Some(c) = doc.style.constants.iter().find(|c| c.name == value) {
        return Some(GotoDefinitionResponse::Scalar(Location {
            uri: uri.clone(),
            range: parser_line_to_lsp_range(c.line),
        }));
    }
    if let Some(project) = project {
        if let Some((path, line)) = project.find_theme_field_location(value) {
            let target_uri = Url::from_file_path(&path).ok()?;
            return Some(GotoDefinitionResponse::Scalar(Location {
                uri: target_uri,
                range: parser_line_to_lsp_range(line),
            }));
        }
    }
    None
}

fn find_component(
    tag: &str,
    project: Option<&ProjectInfo>,
    current_uri: &Url,
) -> Option<GotoDefinitionResponse> {
    let current_dir = current_uri
        .to_file_path()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()));

    let all_dirs = project
        .map(|p| p.root.as_path())
        .into_iter()
        .chain(current_dir.as_deref());

    for dir in all_dirs {
        let files = rsx_transpiler::find_rsx_files(dir);
        if let Some(path) = files
            .iter()
            .find(|p| p.file_stem().and_then(|s| s.to_str()) == Some(tag))
        {
            let target_uri = Url::from_file_path(path).ok()?;
            return Some(GotoDefinitionResponse::Scalar(Location {
                uri: target_uri,
                range: parser_line_to_lsp_range(1),
            }));
        }
    }
    None
}
