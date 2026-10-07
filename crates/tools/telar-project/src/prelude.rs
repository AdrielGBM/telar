//! Which crates a package's `.rsx` tags resolve against besides `telar` and the package itself, as `[telar] prelude` declares them.
//!
//! Generated code opens with `use telar::*;` and `use crate::*;`, and a tag is whatever those two globs put in scope. A component that ships in a crate of its own is in neither, so the project names that crate here and every writer of generated code adds a glob for it between the two.
//!
//! Every writer: the CLI's transpile, the macro's handshake, the editor's live mirror, the golden harness and a `build.rs` all read this key through [`resolve_prelude`], so a package cannot be transpiled against one set of crates by one of them and another by the next.

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// One `[telar] prelude` entry, as generated code spells it: a crate, or a path inside one, whose items are glob-imported into every `.rsx` of the package.
///
/// Only constructed through [`PreludeEntry::parse`], so an entry that exists is one that can be emitted as `use <path>::*;` without producing Rust that does not parse.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
#[serde(try_from = "String")]
pub struct PreludeEntry(String);

impl PreludeEntry {
    /// Reads one entry as written in `telar.toml`. The first segment may be spelled as the package is named in `Cargo.toml` (`telar-components`), and is normalised to the crate name Rust sees (`telar_components`); every segment after it is an identifier as written (`telar_components::prelude`).
    pub fn parse(declared: &str) -> Result<Self, String> {
        let trimmed = declared.trim();
        if trimmed.is_empty() {
            return Err("a `[telar] prelude` entry is empty: name a crate, like \"telar-components\", or a path inside one, like \"telar_components::prelude\"".to_string());
        }
        let mut segments = trimmed.split("::");
        let root = segments.next().unwrap_or_default().replace('-', "_");
        let path: Vec<String> = std::iter::once(root)
            .chain(segments.map(str::to_string))
            .collect();
        if let Some(bad) = path.iter().find(|segment| !is_plain_identifier(segment)) {
            return Err(format!(
                "`[telar] prelude` entry \"{declared}\" is not a Rust path: `{bad}` is not an identifier. Name a crate, like \"telar-components\", or a path inside one, like \"telar_components::prelude\""
            ));
        }
        Ok(Self(path.join("::")))
    }

    /// The path generated code glob-imports, without the trailing `::*`.
    pub fn path(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for PreludeEntry {
    type Error = String;

    fn try_from(declared: String) -> Result<Self, Self::Error> {
        Self::parse(&declared)
    }
}

impl std::fmt::Display for PreludeEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The prelude `<package_dir>/telar.toml` declares, inherited from the workspace's when the package names none and is not a library.
///
/// The one resolver every writer of generated code calls. An unreadable manifest is an error rather than an empty prelude: transpiling against fewer crates than the project declared produces Rust that fails on every tag those crates provide, and reports it against the markup instead of the key that is wrong.
pub fn resolve_prelude(package_dir: &Path) -> Result<Vec<PreludeEntry>, crate::ManifestError> {
    Ok(crate::TelarManifest::load(package_dir)?
        .telar
        .prelude()
        .to_vec())
}

/// One `[telar] prelude` entry, and the `telar.toml` line that declares it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreludeDeclaration {
    pub entry: PreludeEntry,
    pub file: PathBuf,
    /// 1-based.
    pub line: usize,
    /// The character columns of the entry's string, quotes included, within [`Self::line`].
    pub columns: (usize, usize),
}

impl PreludeDeclaration {
    /// The crate the entry's path starts at, as Rust names it.
    pub fn root_crate(&self) -> &str {
        let path = self.entry.path();
        path.split_once("::").map_or(path, |(root, _)| root)
    }
}

/// A `[telar] prelude` entry generated code would glob-import and the package cannot reach, said about the line that declares it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreludeProblem {
    pub declaration: PreludeDeclaration,
    pub message: String,
    pub help: String,
}

/// The prelude [`resolve_prelude`] returns for `package_dir`, each entry paired with where it is written: the package's own `telar.toml`, or the workspace's when the package inherits it.
pub fn prelude_declarations(
    package_dir: &Path,
) -> Result<Vec<PreludeDeclaration>, crate::ManifestError> {
    crate::TelarManifest::load(package_dir)?;
    for file in crate::TelarManifest::files(package_dir) {
        let Ok(content) = std::fs::read_to_string(&file) else {
            continue;
        };
        let spanned: SpannedManifest =
            toml::from_str(&content).map_err(|e| crate::ManifestError::Invalid {
                path: file.clone(),
                message: e.to_string(),
            })?;
        let Some(prelude) = spanned.telar.and_then(|telar| telar.prelude) else {
            continue;
        };
        return Ok(prelude
            .iter()
            .filter_map(|declared| {
                let entry = PreludeEntry::parse(declared.get_ref()).ok()?;
                let span = declared.span();
                let line_start = content[..span.start].rfind('\n').map_or(0, |at| at + 1);
                let column = |at: usize| content[line_start..at].chars().count();
                Some(PreludeDeclaration {
                    entry,
                    file: file.clone(),
                    line: content[..span.start].matches('\n').count() + 1,
                    columns: (column(span.start), column(span.end)),
                })
            })
            .collect());
    }
    Ok(Vec::new())
}

/// Every entry of `package_dir`'s prelude whose crate its `Cargo.toml` does not list under `[dependencies]`.
///
/// A crate generated code globs has to be one the package itself depends on: rustc otherwise reports an unresolved import against every generated file, none of which the author wrote, and the key that caused it goes unnamed. A `Cargo.toml` that cannot be read answers nothing rather than flagging every entry.
pub fn prelude_problems(package_dir: &Path) -> Result<Vec<PreludeProblem>, crate::ManifestError> {
    let declarations = prelude_declarations(package_dir)?;
    let Some(manifest) = std::fs::read_to_string(package_dir.join("Cargo.toml"))
        .ok()
        .and_then(|content| content.parse::<toml::Table>().ok())
    else {
        return Ok(Vec::new());
    };
    let package = manifest
        .get("package")
        .and_then(|package| package.get("name"))
        .and_then(toml::Value::as_str)
        .unwrap_or("this package");
    let normal = dependency_crates(&manifest, "dependencies");
    Ok(declarations
        .into_iter()
        .filter(|declaration| {
            let root = declaration.root_crate();
            !normal.contains(root) && !ALWAYS_LINKED.contains(&root)
        })
        .map(|declaration| {
            let root = declaration.root_crate().to_string();
            let package_name = root.replace('_', "-");
            let elsewhere = ["dev-dependencies", "build-dependencies"]
                .into_iter()
                .find(|table| dependency_crates(&manifest, table).contains(&root));
            let help = match elsewhere {
                Some(table) => format!(
                    "`{root}` is under `[{table}]`, which a `.rsx` cannot reach: move it to `[dependencies]` (`cargo add {package_name}`)"
                ),
                None => format!(
                    "add it with `cargo add -p {package} {package_name}`, or remove the entry"
                ),
            };
            PreludeProblem {
                message: format!(
                    "`[telar] prelude` entry `{}` names `{root}`, which is not a dependency of `{package}`",
                    declaration.entry
                ),
                help,
                declaration,
            }
        })
        .collect())
}

/// Crates every Rust program can name without declaring them.
const ALWAYS_LINKED: [&str; 3] = ["std", "core", "alloc"];

/// The crate names Rust sees for one dependency table of `manifest`, its target-specific copies included. A renamed dependency is named by its key, which is what `package = "…"` exists to choose.
fn dependency_crates(manifest: &toml::Table, table: &str) -> std::collections::HashSet<String> {
    let targets = manifest
        .get("target")
        .and_then(toml::Value::as_table)
        .into_iter()
        .flat_map(|targets| targets.values())
        .filter_map(|target| target.get(table));
    std::iter::once(manifest.get(table))
        .flatten()
        .chain(targets)
        .filter_map(toml::Value::as_table)
        .flat_map(|dependencies| dependencies.keys())
        .map(|key| key.replace('-', "_"))
        .collect()
}

#[derive(Deserialize)]
struct SpannedManifest {
    telar: Option<SpannedTelar>,
}

#[derive(Deserialize)]
struct SpannedTelar {
    prelude: Option<Vec<toml::Spanned<String>>>,
}

/// The problems a single file's `prelude` has beyond what each entry checks on its own.
pub(crate) fn problems(entries: &[PreludeEntry]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    entries
        .iter()
        .filter(|entry| !seen.insert(entry.path()))
        .map(|entry| format!("`[telar] prelude` lists `{entry}` more than once"))
        .collect()
}

fn is_plain_identifier(segment: &str) -> bool {
    let mut chars = segment.chars();
    let starts_well = chars
        .next()
        .is_some_and(|first| first == '_' || first.is_ascii_alphabetic());
    starts_well
        && segment != "_"
        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
        && !KEYWORDS.contains(&segment)
}

/// Rust's strict and reserved keywords, across editions. `crate`, `self` and `super` are among them on purpose: the package's own items are already in scope, and the key exists to name other crates.
const KEYWORDS: &[&str] = &[
    "Self", "abstract", "as", "async", "await", "become", "box", "break", "const", "continue",
    "crate", "do", "dyn", "else", "enum", "extern", "false", "final", "fn", "for", "gen", "if",
    "impl", "in", "let", "loop", "macro", "match", "mod", "move", "mut", "override", "priv", "pub",
    "ref", "return", "self", "static", "struct", "super", "trait", "true", "try", "type", "typeof",
    "unsafe", "unsized", "use", "virtual", "where", "while", "yield",
];

#[cfg(test)]
#[path = "prelude_test.rs"]
mod tests;
