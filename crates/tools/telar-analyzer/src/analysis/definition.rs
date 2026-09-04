//! Go-to-definition for the things `.rsx` names itself: classes, colours and component tags.

use crate::analysis::util::{ViewToken, view_token_at};
use crate::position::parser_line_to_lsp_range;
use crate::project::ProjectInfo;
use lsp_types::{GotoDefinitionResponse, Location, Uri};
use telar_parser::RsxDocument;
use telar_transpiler::is_builtin_tag;

/// Where the thing under the cursor is defined, for the names `.rsx` owns itself.
pub fn goto_definition(
    doc: &RsxDocument,
    source: &str,
    uri: &Uri,
    line: u32,
    character: u32,
    project: Option<&ProjectInfo>,
) -> Option<GotoDefinitionResponse> {
    match view_token_at(source, line, character)? {
        ViewToken::Class(class) => find_class(doc, class, uri),
        // An attribute is declared in the registry, not in a file this can open.
        ViewToken::Attr { .. } => None,
        ViewToken::ColorValue(value) => find_color(value, project),
        // A builtin tag is defined in the framework, not in a file this can open.
        ViewToken::Tag(tag) if !is_builtin_tag(tag) => find_component(tag, project, uri),
        ViewToken::Tag(_) => None,
    }
}

fn find_class(doc: &RsxDocument, name: &str, uri: &Uri) -> Option<GotoDefinitionResponse> {
    let class = doc.style.classes.iter().find(|c| c.name == name)?;
    Some(GotoDefinitionResponse::Scalar(Location {
        uri: uri.clone(),
        range: parser_line_to_lsp_range(class.line),
    }))
}

fn find_color(value: &str, project: Option<&ProjectInfo>) -> Option<GotoDefinitionResponse> {
    if let Some(project) = project
        && let Some((path, line)) = project.find_theme_field_location(value)
    {
        let target_uri = crate::uri::from_path(&path)?;
        return Some(GotoDefinitionResponse::Scalar(Location {
            uri: target_uri,
            range: parser_line_to_lsp_range(line),
        }));
    }
    None
}

fn find_component(
    tag: &str,
    project: Option<&ProjectInfo>,
    current_uri: &Uri,
) -> Option<GotoDefinitionResponse> {
    let current_dir =
        crate::uri::to_path(current_uri).and_then(|p| p.parent().map(|d| d.to_path_buf()));

    let all_dirs = project
        .map(|p| p.component_root.as_path())
        .into_iter()
        .chain(current_dir.as_deref());

    for dir in all_dirs {
        let files = telar_transpiler::find_rsx_files_in_tree(dir);
        if let Some(path) = files
            .iter()
            .find(|p| p.file_stem().and_then(|s| s.to_str()) == Some(tag))
        {
            let target_uri = crate::uri::from_path(path)?;
            return Some(GotoDefinitionResponse::Scalar(Location {
                uri: target_uri,
                range: parser_line_to_lsp_range(1),
            }));
        }
    }
    None
}
