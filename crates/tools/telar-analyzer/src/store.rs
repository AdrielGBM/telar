//! The open documents: the live text, and the last version that parsed.

use lsp_types::Uri;
use std::collections::HashMap;
use std::path::PathBuf;
use telar_diagnostics::Diagnostic;
use telar_parser::{RsxDocument, parse};
use telar_transpiler::SourceMap;

/// The last version of a document that parsed, kept so completion survives a mid-edit buffer.
pub struct ParsedDocument {
    pub source: String,
    pub document: RsxDocument,
}

/// The open documents: each one's live text, and its last good parse.
pub struct Store {
    docs: HashMap<Uri, ParsedDocument>,
    // The current buffer text, updated on every edit even when it fails to parse. `docs` only holds the last good parse (so completion/hover keep working mid-edit), but formatting must operate on the live text — otherwise it could reformat a stale version and clobber the user's edits.
    latest_source: HashMap<Uri, String>,
    live_maps: HashMap<PathBuf, (Uri, SourceMap)>,
}

impl Store {
    pub fn new() -> Self {
        Self {
            docs: HashMap::new(),
            latest_source: HashMap::new(),
            live_maps: HashMap::new(),
        }
    }

    pub fn close(&mut self, uri: &Uri) {
        self.docs.remove(uri);
        self.latest_source.remove(uri);
        self.live_maps.retain(|_, (owner, _)| owner != uri);
    }

    pub fn set_live_map(&mut self, uri: Uri, build_path: PathBuf, map: SourceMap) {
        self.live_maps.insert(build_path, (uri, map));
    }

    /// The source map of the open buffer that generated `build_path`.
    pub fn live_map(&self, build_path: &std::path::Path) -> Option<&SourceMap> {
        self.live_maps.get(build_path).map(|(_, map)| map)
    }

    pub fn get(&self, uri: &Uri) -> Option<&ParsedDocument> {
        self.docs.get(uri)
    }

    /// The latest buffer text for `uri`, regardless of whether it currently parses.
    pub fn latest_source(&self, uri: &Uri) -> Option<&String> {
        self.latest_source.get(uri)
    }

    /// Any currently-open document URI, used to locate the workspace root for `workspace/symbol` (which carries no document of its own).
    pub fn any_uri(&self) -> Option<&Uri> {
        self.latest_source.keys().next()
    }

    /// The classes `document`, the `.rsx` at `path`, imports in its `[style]`: read from the buffer of each module the editor has open, and from disk for the rest, so a class declared a keystroke ago is found the way the next build will find it once saved.
    pub fn style_imports(
        &self,
        path: &std::path::Path,
        document: &RsxDocument,
    ) -> telar_transpiler::ResolvedImports {
        let Some(src_dir) = telar_project::find_package_root(path).map(|root| root.join("src"))
        else {
            return telar_transpiler::ResolvedImports::unresolvable(&document.style);
        };
        telar_transpiler::resolve_style_imports(path, &src_dir, &document.style, &|module| {
            crate::uri::from_path(module)
                .and_then(|uri| self.latest_source(&uri).cloned())
                .or_else(|| std::fs::read_to_string(module).ok())
        })
    }

    pub fn reparse(&mut self, uri: Uri, source: String) -> Vec<Diagnostic> {
        self.latest_source.insert(uri.clone(), source.clone());
        match parse(&source) {
            Ok(document) => {
                self.docs.insert(uri, ParsedDocument { source, document });
                vec![]
            }
            Err(err) => vec![Diagnostic::from(err)],
        }
    }
}

#[cfg(test)]
#[path = "store_test.rs"]
mod tests;
