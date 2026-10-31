use std::path::{Component, Path, PathBuf};

use crate::collect_files_by_ext;

/// Every `*_test.rs` under `package_dir/src` that nothing compiles, each said as a warning naming the file and the two ways to wire it.
///
/// Module discovery never declares a `*_test.rs` (see `is_rust_module_file`), so a test file is compiled only when a module includes it through `#[path]` or a Cargo target names it. A file neither does is skipped by rustc without a word, and the tests in it never run.
pub fn orphan_test_warnings(package_dir: &Path) -> Vec<String> {
    let src_dir = package_dir.join("src");
    let sources = collect_files_by_ext(&src_dir, &["rs"], &|name| !name.starts_with('.'));
    let tests: Vec<&PathBuf> = sources.iter().filter(|path| is_test_file(path)).collect();
    if tests.is_empty() {
        return Vec::new();
    }
    let mut wired: Vec<PathBuf> = sources
        .iter()
        .flat_map(|path| declared_files(path))
        .collect();
    wired.extend(cargo_target_paths(package_dir));
    let mut orphans: Vec<PathBuf> = tests
        .into_iter()
        .filter(|test| !wired.contains(&normalized(test)))
        .cloned()
        .collect();
    orphans.sort();
    orphans
        .iter()
        .map(|test| orphan_warning(package_dir, test))
        .collect()
}

fn is_test_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with("_test.rs"))
}

fn orphan_warning(package_dir: &Path, test: &Path) -> String {
    let shown =
        crate::relative_slashed(test, package_dir).unwrap_or_else(|| test.display().to_string());
    let file_name = test
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    let module = file_name.trim_end_matches("_test.rs");
    format!(
        "{shown} is compiled by nothing, so its tests never run: add `#[cfg(test)] #[path = \"{file_name}\"] mod tests;` to {module}.rs, or a `[[test]]` target with `path = \"{shown}\"` to Cargo.toml"
    )
}

/// The files one source declares as modules: through `#[path = "…"]`, resolved against its directory as rustc does, and through a plain `mod name;`, which rustc looks for beside a crate root or `mod.rs` and under the directory named after any other file.
fn declared_files(source_file: &Path) -> Vec<PathBuf> {
    let Ok(source) = std::fs::read_to_string(source_file) else {
        return Vec::new();
    };
    let dir = source_file.parent().unwrap_or(Path::new(""));
    let module_dir = match is_module_root_file(source_file) {
        true => dir.to_path_buf(),
        false => dir.join(source_file.file_stem().unwrap_or_default()),
    };
    let code: Vec<&str> = source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect();
    let by_path = code
        .iter()
        .flat_map(|line| line.split("#[").skip(1))
        .filter_map(path_value)
        .map(|value| normalized(&dir.join(value)));
    let by_name = code
        .iter()
        .flat_map(|line| plain_module_names(line))
        .map(|name| normalized(&module_dir.join(format!("{name}.rs"))));
    by_path.chain(by_name).collect()
}

fn is_module_root_file(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some("lib.rs" | "main.rs" | "mod.rs")
    )
}

fn plain_module_names(line: &str) -> Vec<&str> {
    let mut words = line.split_whitespace();
    let mut names = Vec::new();
    while let Some(word) = words.next() {
        if word != "mod" {
            continue;
        }
        if let Some(name) = words.next().and_then(|next| next.strip_suffix(';')) {
            names.push(name);
        }
    }
    names
}

/// The string a `path = "…"` inside one attribute assigns, whether bare or inside a `cfg_attr`.
fn path_value(attribute: &str) -> Option<&str> {
    let attribute = attribute.split(']').next()?;
    let mut rest = attribute;
    while let Some(at) = rest.find("path") {
        let before = rest[..at].chars().next_back();
        let after = rest[at + "path".len()..].trim_start();
        rest = &rest[at + "path".len()..];
        if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
            continue;
        }
        let Some(value) = after.strip_prefix('=').map(str::trim_start) else {
            continue;
        };
        let value = value.strip_prefix('"')?;
        return value.split('"').next();
    }
    None
}

/// The `path` of every `[[test]]`, `[[bench]]`, `[[example]]` and `[[bin]]` target of the package's `Cargo.toml`, which cargo compiles as crate roots of their own.
fn cargo_target_paths(package_dir: &Path) -> Vec<PathBuf> {
    let Ok(text) = std::fs::read_to_string(package_dir.join("Cargo.toml")) else {
        return Vec::new();
    };
    let Ok(manifest) = text.parse::<toml::Table>() else {
        return Vec::new();
    };
    ["test", "bench", "example", "bin"]
        .iter()
        .filter_map(|kind| manifest.get(*kind)?.as_array())
        .flatten()
        .filter_map(|target| target.get("path")?.as_str())
        .map(|path| normalized(&package_dir.join(path)))
        .collect()
}

fn normalized(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
#[path = "orphan_tests_test.rs"]
mod tests;
