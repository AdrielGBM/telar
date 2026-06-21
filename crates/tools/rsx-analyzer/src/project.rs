use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub struct ProjectInfo {
    pub root: PathBuf,
    pub theme_type: Option<String>,
    pub theme_fields: HashSet<String>,
}

impl ProjectInfo {
    pub fn find_theme_field_location(&self, field_name: &str) -> Option<(PathBuf, usize)> {
        let type_name = self.theme_type.as_deref()?;
        let struct_marker = format!("struct {type_name}");
        for path in collect_rs_files(&self.root) {
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            let mut in_struct = false;
            let mut depth = 0i32;
            for (i, line) in content.lines().enumerate() {
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
                if let Some(name) = parse_color_field(trimmed) {
                    if name == field_name {
                        return Some((path, i + 1));
                    }
                }
            }
        }
        None
    }

    pub fn discover(file_path: &Path) -> Option<Self> {
        let root = find_project_root(file_path)?;
        let toml_path = root.join("rsx.toml");
        let theme_type = read_theme_type(&toml_path);
        let theme_fields = if let Some(ref type_name) = theme_type {
            scan_theme_fields(&root, type_name)
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

fn find_project_root(start: &Path) -> Option<PathBuf> {
    let mut dir = if start.is_file() {
        start.parent()?
    } else {
        start
    };
    loop {
        if dir.join("rsx.toml").exists() {
            return Some(dir.to_path_buf());
        }
        dir = dir.parent()?;
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

fn scan_theme_fields(root: &Path, type_name: &str) -> HashSet<String> {
    let mut fields = HashSet::new();
    let rs_files = collect_rs_files(root);
    for path in rs_files {
        if let Ok(content) = std::fs::read_to_string(&path) {
            extract_theme_fields(&content, type_name, &mut fields);
        }
    }
    fields
}

fn collect_rs_files(dir: &Path) -> Vec<PathBuf> {
    let mut result = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) {
                    if dir_name == "target" || dir_name == ".rsx" {
                        continue;
                    }
                }
                result.extend(collect_rs_files(&path));
            } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                result.push(path);
            }
        }
    }
    result
}

fn extract_theme_fields(source: &str, type_name: &str, fields: &mut HashSet<String>) {
    let struct_marker = format!("struct {type_name}");
    let mut in_struct = false;
    let mut depth = 0i32;
    for line in source.lines() {
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
        if depth <= 0 && in_struct {
            break;
        }
        if let Some(field) = parse_color_field(trimmed) {
            fields.insert(field);
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
