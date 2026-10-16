//! `use` in `[style]`: the classes another `.rsx` module of the package makes `pub`, found by its module path the way rustc finds an item.
//!
//! A class is a bundle of attributes the transpiler writes onto each element that names it, so the file that uses one has to see its properties while it is transpiled: an import is read from the module's source, not resolved by rustc. Its values are Rust spliced where the class is used, as a `macro_rules!` macro's tokens are, so a path they name has to resolve there too.

use std::path::{Path, PathBuf};

use telar_parser::{StyleClass, StyleImport, StyleImportItems, StyleSection};

/// A class brought in by a `use`, under the name the importing file writes it as.
#[derive(Debug, Clone)]
pub struct ImportedClass {
    pub class: StyleClass,
    /// The `.rsx` that declares it, and the 1-based line of its `pub @name` there.
    pub source: PathBuf,
    pub line: usize,
}

/// Why a class import brings in nothing, on the 1-based line of the `use` that asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportError {
    pub line: usize,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct ResolvedImports {
    pub classes: Vec<ImportedClass>,
    pub errors: Vec<ImportError>,
}

impl ResolvedImports {
    pub fn class_list(&self) -> Vec<StyleClass> {
        self.classes
            .iter()
            .map(|imported| imported.class.clone())
            .collect()
    }

    /// Every import refused, for a file transpiled with no package to look the modules up in.
    pub fn unresolvable(section: &StyleSection) -> Self {
        Self {
            classes: Vec::new(),
            errors: section
                .imports
                .iter()
                .map(|import| ImportError {
                    line: import.line,
                    message: format!(
                        "`use {}` names a module of the package, and this file is transpiled outside one",
                        import.module.join("::")
                    ),
                })
                .collect(),
        }
    }
}

/// Resolves the class imports of the `.rsx` at `rsx_path`, a file under `src_dir`, reading each module it names through `read` — the disk, or an editor's buffers for the files it has open.
pub fn resolve_style_imports(
    rsx_path: &Path,
    src_dir: &Path,
    section: &StyleSection,
    read: &dyn Fn(&Path) -> Option<String>,
) -> ResolvedImports {
    let mut resolved = ResolvedImports::default();
    if section.imports.is_empty() {
        return resolved;
    }
    let Some(current) = module_path(rsx_path, src_dir) else {
        return ResolvedImports::unresolvable(section);
    };
    for import in &section.imports {
        match resolve_one(import, &current, src_dir, read) {
            Ok(classes) => {
                for imported in classes {
                    let name = &imported.class.name;
                    if section.classes.iter().any(|class| &class.name == name) {
                        resolved.errors.push(ImportError {
                            line: import.line,
                            message: format!(
                                "`@{name}` is declared in this `[style]` and imported too: rename one with `as`"
                            ),
                        });
                    } else if resolved
                        .classes
                        .iter()
                        .any(|class| &class.class.name == name)
                    {
                        resolved.errors.push(ImportError {
                            line: import.line,
                            message: format!("`@{name}` is imported twice: rename one with `as`"),
                        });
                    } else {
                        resolved.classes.push(imported);
                    }
                }
            }
            Err(message) => resolved.errors.push(ImportError {
                line: import.line,
                message,
            }),
        }
    }
    resolved
}

fn resolve_one(
    import: &StyleImport,
    current: &[String],
    src_dir: &Path,
    read: &dyn Fn(&Path) -> Option<String>,
) -> Result<Vec<ImportedClass>, String> {
    let shown = import.module.join("::");
    let target = target_module(&import.module, current)
        .ok_or_else(|| format!("`{shown}` reaches above the crate root"))?;
    let (path, source) = module_files(&target, src_dir)
        .into_iter()
        .find_map(|path| read(&path).map(|source| (path, source)))
        .ok_or_else(|| {
            format!("`{shown}` is no `.rsx` module of this crate, so it declares no classes")
        })?;
    let document = telar_parser::parse(&source).map_err(|error| {
        format!(
            "`{shown}` does not parse (line {}: {}), so its classes cannot be read",
            error.line, error.message
        )
    })?;
    let declared = &document.style.classes;
    let imported = |class: &StyleClass, local: &str| ImportedClass {
        class: StyleClass {
            name: local.to_string(),
            ..class.clone()
        },
        source: path.clone(),
        line: class.line,
    };
    match &import.items {
        StyleImportItems::Glob => Ok(declared
            .iter()
            .filter(|class| class.public)
            .map(|class| imported(class, &class.name))
            .collect()),
        StyleImportItems::Names(names) => names
            .iter()
            .map(|name| {
                let class = declared
                    .iter()
                    .find(|class| class.name == name.name)
                    .ok_or_else(|| format!("`{shown}` declares no class `@{}`", name.name))?;
                if !class.public {
                    return Err(format!(
                        "`@{}` is private to `{shown}`: write it `pub @{}` there to share it",
                        name.name, name.name
                    ));
                }
                Ok(imported(class, name.local()))
            })
            .collect(),
    }
}

/// The module path of the `.rsx` at `path` under `src_dir`: its directories and its stem, a `mod.rsx` being its directory's module and a `checkbox.previews.rsx` the `checkbox_previews` beside `checkbox`.
fn module_path(path: &Path, src_dir: &Path) -> Option<Vec<String>> {
    let relative = path.strip_prefix(src_dir).ok()?;
    let mut segments: Vec<String> = relative
        .parent()?
        .iter()
        .map(|part| part.to_string_lossy().into_owned())
        .collect();
    if !telar_project::is_module_root(path) {
        segments.push(telar_project::module_name(path));
    }
    Some(segments)
}

fn target_module(path: &[String], current: &[String]) -> Option<Vec<String>> {
    let mut segments = path.iter().peekable();
    let mut target: Vec<String> = match segments.peek().map(|s| s.as_str()) {
        Some("crate") => {
            segments.next();
            Vec::new()
        }
        Some("self") => {
            segments.next();
            current.to_vec()
        }
        _ => current.to_vec(),
    };
    while segments.peek().is_some_and(|s| *s == "super") {
        segments.next();
        target.pop()?;
    }
    target.extend(segments.cloned());
    (!target.is_empty()).then_some(target)
}

fn module_files(segments: &[String], src_dir: &Path) -> Vec<PathBuf> {
    let dir: PathBuf = segments[..segments.len() - 1].iter().collect();
    let name = &segments[segments.len() - 1];
    vec![
        src_dir.join(&dir).join(format!("{name}.rsx")),
        src_dir
            .join(&dir)
            .join(name)
            .join(telar_project::MODULE_ROOT_FILENAME),
    ]
}

#[cfg(test)]
#[path = "style_import_test.rs"]
mod tests;
