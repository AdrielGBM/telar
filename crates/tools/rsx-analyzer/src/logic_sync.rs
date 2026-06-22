use std::path::{Path, PathBuf};

pub fn lsp_dir(project_root: &Path) -> PathBuf {
    project_root.join(".rsx").join("lsp")
}

pub fn logic_file_path(project_root: &Path, rsx_path: &Path) -> Option<PathBuf> {
    let src_dir = project_root.join("src");
    let stem = rsx_transpiler::relative_stem(rsx_path, &src_dir);
    if stem.is_empty() {
        return None;
    }
    Some(lsp_dir(project_root).join(format!("{stem}.rs")))
}

pub fn remove_logic_file(rsx_path: &Path, project_root: &Path) {
    if let Some(path) = logic_file_path(project_root, rsx_path) {
        let _ = std::fs::remove_file(path);
    }
}

pub(crate) fn ensure_cargo_toml(lsp_dir: &Path, project_root: &Path) {
    let toml_path = lsp_dir.join("Cargo.toml");
    if toml_path.exists() {
        return;
    }

    let rel = relative_path(lsp_dir, project_root);

    let content = format!(
        r#"[package]
name = "rsx-lsp-logic"
version = "0.1.0"
edition = "2024"

[dependencies]
reactive-core = {{ path = "{rel}/crates/reactive/reactive-core" }}
"#,
        rel = rel
    );
    let _ = std::fs::write(toml_path, content);
}

pub(crate) fn relative_path(from: &Path, to: &Path) -> String {
    let from_components: Vec<_> = from.components().collect();
    let to_components: Vec<_> = to.components().collect();

    let common = from_components
        .iter()
        .zip(to_components.iter())
        .take_while(|(a, b)| a == b)
        .count();

    let ups = from_components.len() - common;
    let downs: Vec<_> = to_components[common..]
        .iter()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();

    let mut parts: Vec<String> = (0..ups).map(|_| "..".to_string()).collect();
    parts.extend(downs);

    if parts.is_empty() {
        ".".to_string()
    } else {
        parts.join("/")
    }
}
