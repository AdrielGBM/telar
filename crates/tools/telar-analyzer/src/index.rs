//! Persistent, incremental `.rsx` symbol index for the workspace.
//!
//! `workspace/symbol` and cross-file component `references`/`rename` used to re-read and re-parse every `.rsx` on each query. This caches the per-file facts they need — the component (file stem), its `[style]` `@classes`, and every component `<tag>` usage — and refreshes only the file that changed (the live buffer on each edit, disk on a watched-file event). One scan on first query; O(1) updates after.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use lsp_types::{Location, Position, Range, SymbolInformation, SymbolKind, Uri};
use telar_parser::{header_section, parse};
use telar_transpiler::{is_builtin_tag, is_control_flow_keyword};

use crate::position::Section;
use crate::text::{leading_token, name_range};

/// A component `<tag>` usage in `[view]`/`[preview]` markup: the referenced name and its name-range.
pub struct TagUse {
    pub name: String,
    pub range: Range,
}

/// The indexed facts of one `.rsx` file.
pub struct IndexedFile {
    pub uri: Uri,
    /// The component name — the file stem.
    pub stem: String,
    /// The file name (e.g. `card.rsx`), used as the symbol container.
    pub container: Option<String>,
    /// `(class name, 0-based line)` for every `[style]` `@class`.
    pub classes: Vec<(String, u32)>,
    /// Every component `<tag>` usage in this file.
    pub tags: Vec<TagUse>,
}

pub struct WorkspaceIndex {
    root: PathBuf,
    files: HashMap<PathBuf, IndexedFile>,
}

impl WorkspaceIndex {
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// One-time scan of every `.rsx` under `root` from disk.
    pub fn build(root: &Path) -> Self {
        let mut files = HashMap::new();
        for path in telar_transpiler::find_rsx_files(root) {
            if let Ok(source) = std::fs::read_to_string(&path)
                && let Some(entry) = index_source(&path, &source)
            {
                files.insert(path, entry);
            }
        }
        Self {
            root: root.to_path_buf(),
            files,
        }
    }

    /// Re-index a single file from the given source (the live buffer on an edit).
    pub fn update(&mut self, path: &Path, source: &str) {
        if let Some(entry) = index_source(path, source) {
            self.files.insert(path.to_path_buf(), entry);
        }
    }

    pub fn remove(&mut self, path: &Path) {
        self.files.remove(path);
    }

    /// Re-index from disk (a watched-file change to a non-open `.rsx`); drops the entry if it vanished.
    pub fn refresh_from_disk(&mut self, path: &Path) {
        match std::fs::read_to_string(path) {
            Ok(source) => self.update(path, &source),
            Err(_) => self.remove(path),
        }
    }

    /// `workspace/symbol`: components (file stems) + `@classes`, filtered by a case-insensitive substring.
    pub fn symbols(&self, query: &str) -> Vec<SymbolInformation> {
        let needle = query.to_lowercase();
        let matches = |name: &str| needle.is_empty() || name.to_lowercase().contains(&needle);

        let mut out = Vec::new();
        for entry in self.files.values() {
            if matches(&entry.stem) {
                out.push(symbol(
                    &entry.stem,
                    SymbolKind::MODULE,
                    &entry.uri,
                    0,
                    entry.container.clone(),
                ));
            }
            for (name, line) in &entry.classes {
                if matches(name) {
                    out.push(symbol(
                        &format!("@{name}"),
                        SymbolKind::CLASS,
                        &entry.uri,
                        *line,
                        entry.container.clone(),
                    ));
                }
            }
        }
        out
    }

    /// Cross-file references to component `name`: its defining file (a `(0,0)` marker, the file itself) plus every `<name>` markup tag.
    pub fn component_references(&self, name: &str) -> Vec<Location> {
        let mut out = Vec::new();
        for entry in self.files.values() {
            if entry.stem == name {
                let at = Position {
                    line: 0,
                    character: 0,
                };
                out.push(Location {
                    uri: entry.uri.clone(),
                    range: Range { start: at, end: at },
                });
            }
            for tag in &entry.tags {
                if tag.name == name {
                    out.push(Location {
                        uri: entry.uri.clone(),
                        range: tag.range,
                    });
                }
            }
        }
        out
    }
}

fn index_source(path: &Path, source: &str) -> Option<IndexedFile> {
    let uri = crate::uri::from_path(path)?;
    let stem = path.file_stem().and_then(|s| s.to_str())?.to_string();
    let container = path
        .file_name()
        .and_then(|n| n.to_str())
        .map(str::to_string);

    let mut classes = Vec::new();
    if let Ok(doc) = parse(source) {
        for class in &doc.style.classes {
            classes.push((class.name.clone(), class.line.saturating_sub(1) as u32));
        }
    }

    Some(IndexedFile {
        uri,
        stem,
        container,
        classes,
        tags: scan_tags(source),
    })
}

/// Every component `<tag>` usage in `source` (mirrors `occurrences::component_at`): the leading token of a `[view]`/`[preview]` line when it is a plain identifier that is neither a built-in tag nor a control-flow keyword. Section tracking is inline so the whole scan is a single pass.
fn scan_tags(source: &str) -> Vec<TagUse> {
    let mut out = Vec::new();
    let mut section = Section::Unknown;
    for (i, line) in source.lines().enumerate() {
        if let Some(s) = header_section(line.trim()) {
            section = s;
        }
        if !matches!(section, Section::View | Section::Preview) {
            continue;
        }
        let Some((lead, token)) = leading_token(line) else {
            continue;
        };
        if !token.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            || is_control_flow_keyword(token)
            || is_builtin_tag(token)
        {
            continue;
        }
        out.push(TagUse {
            name: token.to_string(),
            range: name_range(i as u32, line, lead, token.len()),
        });
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

#[cfg(test)]
mod tests {
    use super::*;

    // url::Url::from_file_path rejects unix-style absolute paths on Windows, so tests build platform-valid ones.
    fn abs(unix: &str) -> PathBuf {
        if cfg!(windows) {
            PathBuf::from(format!("C:{}", unix.replace('/', "\\")))
        } else {
            PathBuf::from(unix)
        }
    }

    #[test]
    fn indexes_stem_classes_and_tags() {
        let src =
            "[style]\n@card\n    width: 240\n[view]\ncol @card\n    feature_card icon:\"x\"\n";
        let entry = index_source(&abs("/x/src/home.rsx"), src).unwrap();
        assert_eq!(entry.stem, "home");
        assert_eq!(entry.classes, vec![("card".to_string(), 1)]);
        // `col` is a builtin (not a tag use); `feature_card` is a component reference.
        assert_eq!(entry.tags.len(), 1);
        assert_eq!(entry.tags[0].name, "feature_card");
        assert_eq!(entry.tags[0].range.start.line, 5);
    }

    #[test]
    fn symbols_and_references_query_the_cache() {
        let mut idx = WorkspaceIndex {
            root: abs("/x"),
            files: HashMap::new(),
        };
        let src = "[view]\ncol\n    feature_card\n";
        idx.update(&abs("/x/src/home.rsx"), src);
        idx.update(
            &abs("/x/src/feature_card.rsx"),
            "[view]\ncol\n    text \"hi\"\n",
        );

        // The component definition (its file) + the one `<feature_card>` usage in home.
        let refs = idx.component_references("feature_card");
        assert_eq!(refs.len(), 2);

        // A query matches both the component module and is case-insensitive.
        let syms = idx.symbols("feature");
        assert!(syms.iter().any(|s| s.name == "feature_card"));
    }
}
