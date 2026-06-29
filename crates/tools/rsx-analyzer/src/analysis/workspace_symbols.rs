//! `workspace/symbol`: jump to any `.rsx` component or `@class` across the project.
//!
//! `workspace/symbol` carries no document, so the caller passes the workspace root (found from any
//! open buffer). Each `.rsx` contributes its component (the file stem) and its `[style]` classes,
//! filtered by a case-insensitive substring of the query.

use std::path::Path;

use lsp_types::{Location, Position, Range, SymbolInformation, SymbolKind, Uri};
use rsx_parser::parse;

pub fn workspace_symbols(root: &Path, query: &str) -> Vec<SymbolInformation> {
    let needle = query.to_lowercase();
    let matches = |name: &str| needle.is_empty() || name.to_lowercase().contains(&needle);

    let mut out = Vec::new();
    for path in rsx_transpiler::find_rsx_files(root) {
        let Some(uri) = crate::uri::from_path(&path) else {
            continue;
        };
        let container = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(str::to_string);

        // The component itself — the file stem — at the top of the file.
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str())
            && matches(stem)
        {
            out.push(symbol(stem, SymbolKind::MODULE, &uri, 0, container.clone()));
        }

        let Ok(source) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(doc) = parse(&source) else {
            continue;
        };
        for class in &doc.style.classes {
            if matches(&class.name) {
                out.push(symbol(
                    &format!("@{}", class.name),
                    SymbolKind::CLASS,
                    &uri,
                    class.line.saturating_sub(1) as u32,
                    container.clone(),
                ));
            }
        }
    }
    out
}

fn symbol(
    name: &str,
    kind: SymbolKind,
    uri: &Uri,
    line: u32,
    container_name: Option<String>,
) -> SymbolInformation {
    let at = Position { line, character: 0 };
    #[allow(deprecated)] // `deprecated` is a required-but-deprecated field of SymbolInformation.
    SymbolInformation {
        name: name.to_string(),
        kind,
        tags: None,
        deprecated: None,
        location: Location {
            uri: uri.clone(),
            range: Range { start: at, end: at },
        },
        container_name,
    }
}
