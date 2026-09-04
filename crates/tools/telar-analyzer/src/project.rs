//! The project around the open file: its root, its theme, and its i18n catalogue.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// The project around an open file: its root, its theme tokens and its i18n keys.
pub struct ProjectInfo {
    pub root: PathBuf,
    /// Where to look for *other components*, which is a wider question than [`Self::root`] answers. A crate's `telar.toml` scopes its theme type and its i18n catalog — both genuinely per crate — but components are shared across a workspace, and anchoring their search on the nearest `telar.toml` makes one defined in a sibling crate invisible to completion and go-to-definition. Falls back to `root` outside a workspace.
    pub component_root: PathBuf,
    pub theme_type: Option<String>,
    pub theme_fields: HashSet<String>,
    /// Every key the project's baked catalog defines, or empty when it has no translations.
    pub i18n_keys: HashSet<String>,
}

impl ProjectInfo {
    /// The same for the i18n catalog. `None` when the project has no translations at all, so a project that never opted into i18n gets no key diagnostics rather than one per `t"…"`.
    pub fn catalog_view(&self) -> Option<telar_diagnostics::CatalogView<'_>> {
        if self.i18n_keys.is_empty() {
            return None;
        }
        Some(telar_diagnostics::CatalogView {
            keys: &self.i18n_keys,
        })
    }

    pub fn find_theme_field_location(&self, field_name: &str) -> Option<(PathBuf, usize)> {
        let type_name = self.theme_type.as_deref()?;
        for path in collect_rs_files(&self.root) {
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            let mut location = None;
            scan_theme_fields(&content, type_name, |name, line| {
                if location.is_none() && name == field_name {
                    location = Some(line + 1);
                }
            });
            if let Some(line) = location {
                return Some((path, line));
            }
        }
        None
    }

    pub fn discover(file_path: &Path) -> Option<Self> {
        let root = telar_transpiler::find_telar_root(file_path)?;
        let toml_path = root.join("telar.toml");
        let theme_type = read_theme_type(&toml_path);
        let theme_fields = if let Some(ref type_name) = theme_type {
            scan_project_theme_fields(&root, type_name)
        } else {
            HashSet::new()
        };
        // A malformed catalog is the baker's error to report, not the analyzer's: "no keys known" silences the check rather than flagging every `t"…"`.
        let i18n_keys = telar_transpiler::parse_catalog(&root)
            .ok()
            .flatten()
            .map(|c| c.keys().cloned().collect())
            .unwrap_or_default();
        let component_root =
            telar_transpiler::find_workspace_root(&root).unwrap_or_else(|| root.clone());
        Some(Self {
            root,
            component_root,
            theme_type,
            theme_fields,
            i18n_keys,
        })
    }
}

fn read_theme_type(toml_path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(toml_path).ok()?;
    let table: toml::Value = content.parse().ok()?;
    table
        .get("telar")?
        .get("theme")?
        .as_str()
        .map(|s| s.to_string())
}

fn scan_project_theme_fields(root: &Path, type_name: &str) -> HashSet<String> {
    let mut fields = HashSet::new();
    let rs_files = collect_rs_files(root);
    for path in rs_files {
        if let Ok(content) = std::fs::read_to_string(&path) {
            scan_theme_fields(&content, type_name, |name, _line| {
                fields.insert(name.to_string());
            });
        }
    }
    fields
}

fn collect_rs_files(dir: &Path) -> Vec<PathBuf> {
    // Unlike the transpiler's `.rsx` walk, this prunes build output (`target`, `.rsx`) so theme scanning stays fast and skips generated code.
    telar_transpiler::collect_files_by_ext(dir, "rs", &|name| name != "target" && name != ".telar")
}

/// Scans `source` for `Color` fields inside the `type_name` theme struct, invoking `on_field` with each field name and its 0-based line number in `source`.
fn scan_theme_fields(source: &str, type_name: &str, mut on_field: impl FnMut(&str, usize)) {
    let struct_marker = format!("struct {type_name}");
    let mut in_struct = false;
    let mut depth = 0i32;
    for (i, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        if !in_struct {
            if trimmed.contains(&struct_marker) {
                in_struct = true;
                depth = 0;
            }
            continue;
        }
        depth += trimmed.chars().filter(|&c| c == '{').count() as i32;
        depth -= trimmed.chars().filter(|&c| c == '}').count() as i32;
        if depth <= 0 {
            break;
        }
        if let Some(field) = parse_color_field(trimmed) {
            on_field(&field, i);
        }
    }
}

fn parse_color_field(line: &str) -> Option<String> {
    let line = line.trim_start_matches("pub").trim();
    let line = if line.starts_with('(') {
        line.find(')').map(|i| line[i + 1..].trim()).unwrap_or(line)
    } else {
        line
    };
    let (name, rest) = line.split_once(':')?;
    if rest.trim().starts_with("Color") {
        Some(name.trim().to_string())
    } else {
        None
    }
}
