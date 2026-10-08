//! Which crates a package's `.rsx` tags resolve against besides `telar` and the package itself, as `[telar] prelude` declares them.
//!
//! Generated code opens with `use telar::*;` and `use crate::*;`, and a tag is whatever those two globs put in scope. A component that ships in a crate of its own is in neither, so the project names that crate here and every writer of generated code adds a glob for it between the two.
//!
//! Every writer: the CLI's transpile, the macro's handshake, the editor's live mirror, the golden harness and a `build.rs` all read this key through [`resolve_prelude`], so a package cannot be transpiled against one set of crates by one of them and another by the next.

use std::path::Path;

use serde::Deserialize;

use crate::declared::{DeclaredEntry, EntryProblem, PackageDependencies, declarations};

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
pub type PreludeDeclaration = DeclaredEntry<PreludeEntry>;

impl DeclaredEntry<PreludeEntry> {
    /// The crate the entry's path starts at, as Rust names it.
    pub fn root_crate(&self) -> &str {
        let path = self.entry.path();
        path.split_once("::").map_or(path, |(root, _)| root)
    }
}

/// A `[telar] prelude` entry generated code would glob-import and the package cannot reach, said about the line that declares it.
pub type PreludeProblem = EntryProblem<PreludeEntry>;

/// The prelude [`resolve_prelude`] returns for `package_dir`, each entry paired with where it is written: the package's own `telar.toml`, or the workspace's when the package inherits it.
pub fn prelude_declarations(
    package_dir: &Path,
) -> Result<Vec<PreludeDeclaration>, crate::ManifestError> {
    declarations(package_dir, |telar| telar.prelude, PreludeEntry::parse)
}

/// Every entry of `package_dir`'s prelude whose crate its `Cargo.toml` does not list under `[dependencies]`.
///
/// A crate generated code globs has to be one the package itself depends on: rustc otherwise reports an unresolved import against every generated file, none of which the author wrote, and the key that caused it goes unnamed. A `Cargo.toml` that cannot be read answers nothing rather than flagging every entry.
pub fn prelude_problems(package_dir: &Path) -> Result<Vec<PreludeProblem>, crate::ManifestError> {
    let declarations = prelude_declarations(package_dir)?;
    let Some(dependencies) = PackageDependencies::read(package_dir) else {
        return Ok(Vec::new());
    };
    Ok(declarations
        .into_iter()
        .filter(|declaration| !dependencies.reaches(declaration.root_crate()))
        .map(|declaration| {
            let root = declaration.root_crate().to_string();
            PreludeProblem {
                message: format!(
                    "`[telar] prelude` entry `{}` names `{root}`, which is not a dependency of `{}`",
                    declaration.entry, dependencies.package
                ),
                help: dependencies.missing_help(&root, "a `.rsx`"),
                declaration,
            }
        })
        .collect())
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

pub(crate) fn is_plain_identifier(segment: &str) -> bool {
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
