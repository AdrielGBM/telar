use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub struct ProjectInfo {
    pub root: PathBuf,
    pub theme_type: Option<String>,
    pub theme_fields: HashSet<String>,
}

impl ProjectInfo {
    /// A filesystem-free borrow of the theme data for `rsx_diagnostics::semantic_diagnostics`.
    pub fn theme_view(&self) -> rsx_diagnostics::ThemeView<'_> {
        rsx_diagnostics::ThemeView {
            theme_type: self.theme_type.as_deref(),
            theme_fields: &self.theme_fields,
        }
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
        let root = rsx_workspace::find_rsx_root(file_path)?;
        let toml_path = root.join("rsx.toml");
        let theme_type = read_theme_type(&toml_path);
        let theme_fields = if let Some(ref type_name) = theme_type {
            scan_project_theme_fields(&root, type_name)
        } else {
            HashSet::new()
        };
        Some(Self {
            root,
            theme_type,
            theme_fields,
        })
    }
}

fn read_theme_type(toml_path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(toml_path).ok()?;
    let table: toml::Value = content.parse().ok()?;
    table
        .get("rsx")?
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
    rsx_transpiler::collect_files_by_ext(dir, "rs", &|name| name != "target" && name != ".rsx")
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
