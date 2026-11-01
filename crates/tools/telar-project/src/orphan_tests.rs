use std::path::{Component, Path, PathBuf};

use crate::collect_files_by_ext;
use crate::rust_source::{self, Token};

const TEST_SUFFIX: &str = ".test.rs";

/// Every `*.test.rs` under `package_dir/src` that nothing compiles, each said as a warning naming the file and the two ways to wire it.
///
/// Module discovery never declares a `*.test.rs`, whose name is no identifier, so a test file is compiled only when a module includes it through `#[path]` or a Cargo target names it. A file neither does is skipped by rustc without a word, and the tests in it never run.
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
        .is_some_and(|name| name.ends_with(TEST_SUFFIX))
}

fn orphan_warning(package_dir: &Path, test: &Path) -> String {
    let shown =
        crate::relative_slashed(test, package_dir).unwrap_or_else(|| test.display().to_string());
    let file_name = test
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    let module = file_name.split('.').next().unwrap_or_default();
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
    let tokens = rust_source::tokens(&source);
    let by_path = attribute_paths(&tokens)
        .into_iter()
        .map(|value| normalized(&dir.join(value)));
    let by_name = plain_module_names(&tokens)
        .into_iter()
        .map(|name| normalized(&module_dir.join(format!("{name}.rs"))));
    by_path.chain(by_name).collect()
}

fn is_module_root_file(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some("lib.rs" | "main.rs" | "mod.rs")
    )
}

fn plain_module_names(tokens: &[Token]) -> Vec<&str> {
    tokens
        .windows(3)
        .filter(|decl| decl[0].is_word("mod") && decl[2].is_punct(';'))
        .filter_map(|decl| decl[1].ident())
        .collect()
}

/// The string every `path = "…"` inside an attribute assigns, whether bare or inside a `cfg_attr`.
fn attribute_paths(tokens: &[Token]) -> Vec<&str> {
    let mut values = Vec::new();
    let mut brackets: Option<usize> = None;
    for (at, token) in tokens.iter().enumerate() {
        let Some(open) = brackets else {
            let bracket_at = match tokens.get(at + 1) {
                Some(next) if next.is_punct('!') => at + 2,
                _ => at + 1,
            };
            if token.is_punct('#') && tokens.get(bracket_at).is_some_and(|t| t.is_punct('[')) {
                brackets = Some(0);
            }
            continue;
        };
        if token.is_punct('[') {
            brackets = Some(open + 1);
        } else if token.is_punct(']') {
            brackets = (open > 1).then_some(open - 1);
        } else if token.is_word("path")
            && tokens.get(at + 1).is_some_and(|next| next.is_punct('='))
            && let Some(value) = tokens.get(at + 2).and_then(Token::string)
        {
            values.push(value);
        }
    }
    values
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
#[path = "orphan_tests.test.rs"]
mod tests;
