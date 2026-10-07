//! File-discovery utilities: walking a source tree for `.rsx`/`.rs` files and deriving their output stems and mirrored `.rs` paths.

use std::collections::HashSet;
use std::fmt::Write;
use std::path::{Component, Path, PathBuf};

/// Recursively collects files with any of `extensions` under `root`, descending into a subdirectory only when `keep_dir` returns true for its name. The result is sorted.
///
/// A `root` that names a file is that file, if it matches — so a caller taking paths from a command line does not need a second walk for the case where the user pointed at one thing.
pub fn collect_files_by_ext(
    root: &Path,
    extensions: &[&str],
    keep_dir: &dyn Fn(&str) -> bool,
) -> Vec<PathBuf> {
    fn matches(path: &Path, extensions: &[&str]) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|ext| extensions.contains(&ext))
    }
    fn walk(
        dir: &Path,
        extensions: &[&str],
        keep_dir: &dyn Fn(&str) -> bool,
        result: &mut Vec<PathBuf>,
    ) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let skip = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|name| !keep_dir(name));
                if !skip {
                    walk(&path, extensions, keep_dir, result);
                }
            } else if matches(&path, extensions) {
                result.push(path);
            }
        }
    }

    if root.is_file() {
        return match matches(root, extensions) {
            true => vec![root.to_path_buf()],
            false => Vec::new(),
        };
    }
    let mut result = Vec::new();
    walk(root, extensions, keep_dir, &mut result);
    result.sort();
    result
}

/// Every `.rsx` under `dir`, recursively.
pub fn find_rsx_files(dir: &Path) -> Vec<PathBuf> {
    collect_files_by_ext(dir, &["rsx"], &|_| true)
}

/// Every `.rsx` under a whole workspace, skipping what a build produced rather than a person: `target/` and any dot-directory (which is where the generated `.telar/` tree lives). [`find_rsx_files`] descends into everything, which is right for one crate's `src/` and ruinous one directory up — a workspace root holds a `target/` that dwarfs the sources, and this runs on every completion request.
pub fn find_rsx_files_in_tree(root: &Path) -> Vec<PathBuf> {
    collect_files_by_ext(root, &["rsx"], &|name| {
        name != "target" && !name.starts_with('.')
    })
}

/// The directory that baked `src:"..."` asset paths resolve against: `[telar] assets` in `telar.toml` (default `"assets"`), joined onto the package root — so assets live in one place (e.g. `./assets`) regardless of which `.rsx` references them, instead of being tied to each `.rsx`'s own directory.
pub fn assets_root(package_root: &Path) -> PathBuf {
    crate::TelarManifest::load_or_default(package_root)
        .telar
        .assets_root(package_root)
}

/// The directory under a flavour's generated directory holding one file per discovered subdirectory of `src/`.
pub const MODULE_TREE_DIR: &str = "__modules";

/// One file of a package's module tree: a site's declarations, a directory's children, or the children a `mod.rsx` includes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModuleTreeFile {
    pub(crate) path: PathBuf,
    pub(crate) content: String,
}

/// Every file a package's placement sites `include!` or declare, for one flavour: derived from the source tree alone, written by `cargo telar transpile`, and compared by the macro, which writes nothing.
///
/// Each `#[path]` and `include!` in it is relative to the file holding it, which is how rustc and rust-analyzer resolve both — a module declared in an `include!`d file resolves against that file's directory, and so does a `#[path]` module's own children. So the tree stays valid wherever the package is unpacked, and the content is the same on every machine, which is what lets a published library be checked against the copy it ships.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModuleTree {
    files: Vec<ModuleTreeFile>,
}

impl ModuleTree {
    /// The tree every placement site under `src_dir` wires, declaring the `.rsx` output under `generated_dir`, with each site's declarations in `<site>/.telar/<site_file_name>`.
    pub fn discover(src_dir: &Path, generated_dir: &Path, site_file_name: &str) -> Self {
        let mut files = Vec::new();
        for site in placement_sites(src_dir) {
            let holder = site.join(SITE_DIR);
            let (declarations, children) = discover_rust_modules(src_dir, &site, generated_dir);
            files.extend(children);
            files.push(ModuleTreeFile {
                path: holder.join(site_file_name),
                content: declarations,
            });
        }
        Self { files }
    }

    /// Writes every file whose content differs, returning every path the tree holds for the caller's stale-output sweep.
    pub fn write(&self) -> std::io::Result<Vec<PathBuf>> {
        for file in &self.files {
            if let Some(parent) = file.path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            crate::write_if_changed_atomic(&file.path, &file.content)?;
        }
        Ok(self.files.iter().map(|file| file.path.clone()).collect())
    }

    /// The first file that is missing on disk or holds something else, which is a tree written before the source tree last changed.
    pub fn first_difference(&self) -> Option<&Path> {
        self.files
            .iter()
            .find(|file| {
                std::fs::read_to_string(&file.path).ok().as_deref() != Some(file.content.as_str())
            })
            .map(|file| file.path.as_path())
    }
}

/// Declares `pub mod` for every `.rsx` and every hand-written `.rs` module mirroring the `src_dir` tree, so a package needs no `mod` statements of its own. Returns the declarations for the site at `from_dir` and, for each discovered subdirectory, a generated file under the flavour's [`MODULE_TREE_DIR`] holding that directory's children. Skips the crate roots (`lib.rs`, `main.rs`), directories with no `.rs` under them (asset- or markup-only dirs), and any name the site declares itself. A directory that has its own `mod.rs` is declared but not descended into, so it stays hand-managed — the escape hatch for opting a subtree out of discovery.
///
/// Every module — top-level file, subdirectory, and the children inside each generated file — is a *file-based* `#[path] pub mod` (the exact shape the `.rsx` build files use). It deliberately never emits an inline `mod dir { … }` block: rust-analyzer mis-resolves a `#[path]` attribute on a module nested inside an inline block produced by a proc macro, string-joining the inline module's name onto the child's path and failing to find it (E0583), while rustc joins those pieces with real path semantics. Routing every directory through a real generated file (`#[path = "…/core.rs"] pub mod core;`, its children flat inside that file) keeps the analyzer and the compiler in step.
///
/// **`from_dir` is where the macro was invoked, which is not always `src_dir`.** A module declares its own children, so `rsx_modules!()` in `app/editor/mod.rs` places the `.rsx` files of `app/editor` — declaring `pub mod app;` there instead would name an ancestor of the file doing the declaring, which is a cycle. The generated `.rs` a `.rsx` compiles to still mirrors the whole `src` tree, so the walk carries the prefix from `src_dir` even when it starts below it.
pub(crate) fn discover_rust_modules(
    src_dir: &Path,
    from_dir: &Path,
    generated_dir: &Path,
) -> (String, Vec<ModuleTreeFile>) {
    let prefix = from_dir
        .strip_prefix(src_dir)
        .unwrap_or(Path::new(""))
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("__");
    let mut reserved = hand_written_modules(from_dir);
    if from_dir == src_dir {
        // `src/bin/` is cargo's: every file in it is a crate root of its own. Declared as a module here, each binary's `fn main` would be compiled into the library that declared it.
        reserved.insert("bin".to_owned());
    }
    let mut walk = TreeWalk {
        package_dir: src_dir.parent().unwrap_or(src_dir),
        modtree_dir: generated_dir.join(MODULE_TREE_DIR),
        generated_dir,
        files: Vec::new(),
    };
    let mut out = String::new();
    walk.emit_children(
        from_dir,
        &prefix,
        &from_dir.join(SITE_DIR),
        &reserved,
        &mut out,
    );
    (out, walk.files)
}

/// What one walk over the source tree is writing into, and the files it has produced so far.
struct TreeWalk<'a> {
    /// What a path in a diagnostic is said relative to, so the tree reads the same wherever the package sits.
    package_dir: &'a Path,
    modtree_dir: PathBuf,
    generated_dir: &'a Path,
    files: Vec<ModuleTreeFile>,
}

/// The module names the site's own file already declares, which the discovered tree leaves alone. Redeclaring one is `E0428`, and `mod menu;` written by hand next to a `pub mod menu;` written here would also be a visibility the author did not ask for.
///
/// Only the site's own file is read, and only the names at its top level: everything below is written into generated files, where nothing of the author's can collide. Reached by stripping line comments and taking the identifier after each `mod`, which also catches an inline `mod tests { … }`.
fn hand_written_modules(site_dir: &Path) -> HashSet<String> {
    ["lib.rs", "main.rs", "mod.rs"]
        .iter()
        .filter_map(|name| std::fs::read_to_string(site_dir.join(name)).ok())
        .flat_map(|source| declared_module_names(&source))
        .collect()
}

fn declared_module_names(source: &str) -> Vec<String> {
    let mut names = Vec::new();
    for line in source.lines() {
        let code = line.split("//").next().unwrap_or("");
        let mut words = code.split_whitespace();
        while let Some(word) = words.next() {
            if word != "mod" {
                continue;
            }
            let Some(next) = words.next() else { continue };
            let name = next.trim_end_matches([';', '{']);
            if crate::naming::is_ident(name) {
                names.push(name.to_owned());
            }
        }
    }
    names
}

impl TreeWalk<'_> {
    /// Appends the `#[path] pub mod` declarations for the direct children of `dir` to `out`, the content of a file in `holder`. A subdirectory's own children go into a generated file under the module tree directory (named by the flattened module path, e.g. `core__widgets.rs`, so sibling directories never collide) that the emitted `pub mod` then points at.
    fn emit_children(
        &mut self,
        dir: &Path,
        flat_prefix: &str,
        holder: &Path,
        hand_written: &HashSet<String>,
        out: &mut String,
    ) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        paths.sort();
        for path in paths {
            let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if file_name.starts_with('.') {
                continue;
            }
            let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            if !crate::naming::is_ident(name) {
                continue;
            }
            // A name the site declares itself keeps the author's declaration, visibility included; the diagnostics below still run, because a `mod.rs` sitting on unplaced `.rsx` is a problem whoever declared the module.
            let declared_here = !hand_written.contains(name);
            if path.is_dir() {
                let mod_rs = path.join("mod.rs");
                let flat = match flat_prefix.is_empty() {
                    true => name.to_string(),
                    false => format!("{flat_prefix}__{name}"),
                };
                if path.join(MODULE_ROOT_FILENAME).is_file() {
                    if declared_here {
                        self.module_root(&path, &flat, holder, out);
                    }
                } else if mod_rs.exists() {
                    // A `mod.rs` places its own `.rsx` children by invoking `rsx_modules!()`, the only place they can be declared from — an outside module cannot add items to one. Saying so beats a "cannot find" about a file that is plainly there.
                    if dir_has_rsx(&path) && !file_places_its_own(&mod_rs) {
                        let _ = writeln!(
                            out,
                            "compile_error!(\"{} holds `.rsx` files and a `mod.rs`, so only that file can place them: add `telar::rsx_modules!();` to it, or give the directory a `mod.rsx` and let telar own the module\");",
                            self.shown(&path)
                        );
                    }
                    if declared_here {
                        out.push_str(&mod_decl(name, &mod_rs, holder));
                    }
                } else if declared_here && dir_has_rust_module(&path) {
                    let gen_file = self.modtree_dir.join(format!("{flat}.rs"));
                    let gen_holder = self.modtree_dir.clone();
                    let mut body = String::new();
                    self.emit_children(&path, &flat, &gen_holder, &HashSet::new(), &mut body);
                    out.push_str(&mod_decl(name, &gen_file, holder));
                    self.files.push(ModuleTreeFile {
                        path: gen_file,
                        content: body,
                    });
                }
            } else if is_rust_module_file(&path) {
                if declared_here {
                    out.push_str(&mod_decl(name, &path, holder));
                }
            } else if is_module_root(&path) {
                // Its own directory's module, declared by the parent and generated with the children included; as a sibling here it would be `pub mod mod;`.
            } else if declared_here && path.extension().and_then(|e| e.to_str()) == Some("rsx") {
                // A `.rsx` is a module where the file sits, so `shared/components/card.rsx` is `crate::shared::components::card`. Flattening to the crate root meant two files could not share a basename.
                out.push_str(&mod_decl(
                    name,
                    &rsx_output(self.generated_dir, flat_prefix, name),
                    holder,
                ));
            }
        }
    }

    /// Declares a directory whose `mod.rsx` makes it telar's, and produces the file that module's `include!` reads: its children, plus the hand-written `mod.rs` when the directory keeps one.
    ///
    /// The children cannot come from the transpiler — a `.rsx` is transpiled knowing nothing but itself — and they cannot be added from outside either, since a module takes items only from its own file. The generated module leaves an `include!` and this fills it.
    fn module_root(&mut self, dir: &Path, flat: &str, holder: &Path, out: &mut String) {
        let Some(name) = dir.file_name().and_then(|n| n.to_str()) else {
            return;
        };
        let module_file = rsx_output(self.generated_dir, flat, "mod");
        let children_file = module_file.with_file_name(MODULE_CHILDREN_FILENAME);
        let children_holder = children_file
            .parent()
            .unwrap_or(self.generated_dir)
            .to_path_buf();
        let mut body = String::new();
        self.emit_children(
            dir,
            flat,
            &children_holder,
            &hand_written_modules(dir),
            &mut body,
        );
        let mod_rs = dir.join("mod.rs");
        if mod_rs.is_file() {
            if file_places_its_own(&mod_rs) {
                let _ = writeln!(
                    body,
                    "compile_error!(\"{} has a `mod.rsx`, so telar places this directory: remove the `telar::rsx_modules!()` from its `mod.rs`\");",
                    self.shown(dir)
                );
            }
            match leading_inner_attribute(&mod_rs) {
                Some(attr) => {
                    let _ = writeln!(
                        body,
                        "compile_error!(\"{} starts with `{}`, and an included file cannot carry an inner attribute: move it to the `[logic]` of this directory's `mod.rsx`\");",
                        self.shown(&mod_rs),
                        attr.escape_debug()
                    );
                }
                // `include!` rather than a `#[path] mod`, because the two files are one module: the `mod.rsx` holds what only a module's own file can hold, and this holds the Rust the author kept in `mod.rs`.
                None => {
                    let _ = writeln!(
                        body,
                        "include!({:?});",
                        relative_path(&children_holder, &mod_rs)
                    );
                }
            }
        }
        self.files.push(ModuleTreeFile {
            path: children_file,
            content: body,
        });
        out.push_str(&mod_decl(name, &module_file, holder));
    }

    /// `path` as a diagnostic names it: from the package root, `/`-separated, so the same tree is generated on every machine.
    fn shown(&self, path: &Path) -> String {
        path.strip_prefix(self.package_dir)
            .map(slash_separated)
            .unwrap_or_else(|_| path.to_string_lossy().into_owned())
    }
}

/// Deletes every `.rs` file under `output_dir` that `written` does not list — the leftovers of a renamed or deleted `.rsx`, a directory that lost its last hand-written `.rs`, or a dropped i18n catalog. Also deletes any `.rs.map` sidecar whose `.rs` is not in the written set. Only ever recurses through real subdirectories reached from `output_dir` itself (a symlink is skipped, not followed), so every path it can act on is provably inside the generated tree; it never removes a directory or a non-`.rs`/`.rs.map` file. Best-effort: a removal failure just leaves that orphan for next time.
pub fn prune_stale_generated(output_dir: &Path, written: &HashSet<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(output_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        let path = entry.path();
        if file_type.is_dir() {
            prune_stale_generated(&path, written);
        } else if file_type.is_file() {
            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if file_name.ends_with(".rs") && !file_name.ends_with(".rs.map") {
                if !written.contains(&path) {
                    let _ = std::fs::remove_file(&path);
                    let map_path = path.with_extension("rs.map");
                    let _ = std::fs::remove_file(&map_path);
                }
            } else if file_name.ends_with(".rs.map") {
                let rs_path = path.with_file_name(
                    file_name
                        .strip_suffix(".map")
                        .map(|s| s.to_string())
                        .unwrap_or_default(),
                );
                if !written.contains(&rs_path) {
                    let _ = std::fs::remove_file(&path);
                }
            }
        }
    }
}

/// A `pub mod` declaration pinned to `file` by a `#[path]` relative to `holder`, the directory of the file the declaration is written into. `{:?}` renders the path as an escaped Rust string literal.
fn mod_decl(name: &str, file: &Path, holder: &Path) -> String {
    format!(
        "#[path = {:?}] pub mod {name};\n",
        relative_path(holder, file)
    )
}

/// `to` relative to `from_dir`, `/`-separated whatever the platform, as rustc reads a `#[path]` or an `include!` in a file in `from_dir`. Absolute only when the two share no root at all, which is two drives on Windows.
pub(crate) fn relative_path(from_dir: &Path, to: &Path) -> String {
    let from: Vec<Component> = from_dir.components().collect();
    let target: Vec<Component> = to.components().collect();
    let common = from.iter().zip(&target).take_while(|(a, b)| a == b).count();
    if common == 0 && to.is_absolute() {
        return to.to_string_lossy().into_owned();
    }
    std::iter::repeat_n("..".to_string(), from.len() - common)
        .chain(
            target[common..]
                .iter()
                .map(|part| part.as_os_str().to_string_lossy().into_owned()),
        )
        .collect::<Vec<_>>()
        .join("/")
}

fn slash_separated(path: &Path) -> String {
    path.components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// A `.rs` file that is a declarable module: not a crate root, not a `mod.rs` (the latter is the module root of its own directory, not a sibling child), and not a `*_test.rs`.
///
/// A `*_test.rs` holds the `#[cfg(test)]` body of the module it sits beside, which declares it itself with `#[path]`. Declaring it here too would make it a *sibling* of that module rather than a child, so its `use super::*` would reach the parent directory and every private item it tests would read as inaccessible.
fn is_rust_module_file(path: &Path) -> bool {
    if path.extension().and_then(|e| e.to_str()) != Some("rs") {
        return false;
    }
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    !matches!(name, "lib.rs" | "main.rs" | "mod.rs") && !name.ends_with("_test.rs")
}

/// Whether a directory earns a module node: it holds a declarable `.rs`, a `mod.rs`, or a `.rsx` — a directory of nothing but markup is still a module once its `.rsx` files live where they sit.
fn dir_has_rust_module(dir: &Path) -> bool {
    collect_files_by_ext(dir, &["rs"], &|_| true)
        .iter()
        .any(|p| is_rust_module_file(p) || p.file_name().and_then(|n| n.to_str()) == Some("mod.rs"))
        || !collect_files_by_ext(dir, &["rsx"], &|_| true).is_empty()
}

/// Whether the file — a crate root or a `mod.rs` — invokes the macro that places its own directory.
fn file_places_its_own(file: &Path) -> bool {
    std::fs::read_to_string(file).is_ok_and(|src| places_its_own_rsx(&src))
}

/// Whether `source` invokes the placement macro, rather than naming it in prose. The call has to open its delimiter and survive having line comments stripped: a bare substring was harmless while it only decided whether to suppress a diagnostic, and stopped being so once the same answer decides which directories are placement sites — telar's own sandbox has two files whose comments mention `app!`.
fn places_its_own_rsx(source: &str) -> bool {
    source
        .lines()
        .filter_map(|line| line.split("//").next())
        .any(|code| code.contains("rsx_modules!(") || code.contains("app!("))
}

/// Whether any `.rsx` sits under `dir`, which is what makes a directory with no `mod.rs` worth declaring even when hand-written modules are the crate's own business.
fn dir_has_rsx(dir: &Path) -> bool {
    !collect_files_by_ext(dir, &["rsx"], &|_| true).is_empty()
}

/// Where the transpiler wrote a `.rsx`'s Rust. The generated tree mirrors the source tree, and `flat_prefix` is that same path with `__` between the segments, so the two agree by construction.
fn rsx_output(generated_dir: &Path, flat_prefix: &str, name: &str) -> PathBuf {
    let mut out = generated_dir.to_path_buf();
    for segment in flat_prefix.split("__").filter(|s| !s.is_empty()) {
        out.push(segment);
    }
    out.join(format!("{name}.rs"))
}

/// The name the component in `path` is generated under: its file stem.
///
/// A `.rsx` is a module where its file sits, so two files may share a basename — they are different modules, which is what a path-flattened name existed to work around. One function rather than each caller taking the stem itself: the editor and the golden harness both have to agree with the macro on this exactly, and they silently drifted apart when the flattening went.
pub fn component_name(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Derives the output `.rs` path (relative to the output root) for a `.rsx` file by mirroring its location under `src_dir`, so files in different directories never collide (e.g. `src/components/button.rsx` -> `components/button.rs`). Used for the transpiler's `.telar/build/` output. Returns `None` for files outside `src_dir`: those are never transpiled, so they have no place in the build tree — and flattening their absolute path would escape the output root entirely.
pub fn relative_output_path(path: &Path, src_dir: &Path) -> Option<PathBuf> {
    let rel = path.strip_prefix(src_dir).ok()?;
    if rel.as_os_str().is_empty() {
        return None;
    }
    Some(rel.with_extension("rs"))
}

/// The `.rsx` a generated `.rs` came from: `<crate>/.telar/<flavour>/<rel>.rs` → `<crate>/src/<rel>.rsx`, the inverse of [`relative_output_path`] and [`generated_dir`](crate::generated_dir) together.
///
/// Beside the forward mapping because it *is* the forward mapping read backwards, and because it was written twice: `cargo-telar` projects a rustc diagnostic onto the line an author wrote, and the language server classifies a go-to-definition target the same way. The second copy recognised only `build`, so a session whose last build was `cargo telar dev` — writing `build-hot` — had every jump into generated code land there instead of on the `.rsx`.
///
/// Component-based rather than string matching, so a Windows path with mixed separators still classifies.
pub fn source_for_generated(generated: &Path) -> Option<PathBuf> {
    let (package_root, rel) = split_at_generated_dir(generated)?;
    Some(package_root.join("src").join(rel.with_extension("rsx")))
}

/// Whether `path` is generated output rather than something a person wrote.
pub fn is_generated_output(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()) == Some("rs")
        && split_at_generated_dir(path).is_some()
}

/// Splits `<crate>/.telar/<flavour>/<rel>.rs` into its package root and the path under the flavour directory.
///
/// Every flavour, from [`BuildFlavour::DIR_NAMES`](crate::BuildFlavour::DIR_NAMES) — a fifth one added there is recognised here without being told.
fn split_at_generated_dir(path: &Path) -> Option<(PathBuf, PathBuf)> {
    let parts: Vec<Component> = path.components().collect();
    let at = parts.windows(2).position(|pair| {
        pair[0].as_os_str() == ".telar"
            && crate::BuildFlavour::DIR_NAMES
                .iter()
                .any(|name| pair[1].as_os_str() == *name)
    })?;
    let rel: PathBuf = parts[at + 2..].iter().collect();
    if rel.as_os_str().is_empty() {
        return None;
    }
    Some((parts[..at].iter().collect(), rel))
}

/// The directory a placement site's generated declarations live in, inside the source directory that holds the invocation. Named like the package-level one, and already covered by the `.telar/` line every telar project has in its `.gitignore`.
pub const SITE_DIR: &str = ".telar";

/// What an invocation of the placement macro expands to, whichever module it was written in.
///
/// This is what tells the sites apart, and it has to be the compiler's job rather than the macro's: a proc macro cannot see where it was invoked. `Span::local_file()` answers under rustc and returns `None` under rust-analyzer — whose proc-macro bridge only carries that callback over a protocol no released toolchain speaks — so a macro that guesses the crate root there makes a `mod.rs` declare a module whose file is itself, and the analyzer walks that until the machine is out of memory. A relative `include!` is resolved against the file holding the call by both, without either being asked to know anything.
pub fn site_include_path(flavour: crate::BuildFlavour) -> String {
    format!("{SITE_DIR}/{}", flavour.site_file_name())
}

/// Every directory a crate places `.rsx` from: `src_dir`, plus each directory whose `mod.rs` invokes the placement macro — the only file that can declare that directory's children.
pub fn placement_sites(src_dir: &Path) -> Vec<PathBuf> {
    let mut sites = vec![src_dir.to_path_buf()];
    collect_placement_sites(src_dir, &mut sites);
    sites
}

fn collect_placement_sites(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        if !path.is_dir()
            || path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with('.'))
        {
            continue;
        }
        let mod_rs = path.join("mod.rs");
        // A `mod.rsx` makes the directory telar's, placed from the generated module: an invocation here would be a second placer, which is what `write_module_root` refuses.
        if mod_rs.is_file()
            && file_places_its_own(&mod_rs)
            && !path.join(MODULE_ROOT_FILENAME).is_file()
        {
            out.push(path.clone());
        }
        collect_placement_sites(&path, out);
    }
}

/// Files that invoke the placement macro from where it cannot place. A module declares its children relative to the directory named after its own file, and an `include!` resolves relative to the file itself; the two agree only for a crate root and a `mod.rs`, so anywhere else the invocation would pull in its parent's declarations.
pub fn stray_placement_files(src_dir: &Path) -> Vec<PathBuf> {
    collect_files_by_ext(src_dir, &["rs"], &|name| !name.starts_with('.'))
        .into_iter()
        .filter(|path| {
            !matches!(
                path.file_name().and_then(|n| n.to_str()),
                Some("lib.rs" | "main.rs" | "mod.rs")
            ) && std::fs::read_to_string(path).is_ok_and(|src| places_its_own_rsx(&src))
        })
        .collect()
}

/// Whether anything in the package invokes the placement macro — a crate root, or a `mod.rs` placing its own directory — which is what decides whether it needs a module tree at all.
pub fn invokes_placement_macro(src_dir: &Path) -> bool {
    ["lib.rs", "main.rs"]
        .iter()
        .any(|root| file_places_its_own(&src_dir.join(root)))
        || placement_sites(src_dir).len() > 1
}

/// Deletes the site files of directories that no longer place their own `.rsx` — a `mod.rs` that dropped its invocation, or a directory that lost its last `.rsx`. Only ever removes a `.rs` inside a `.telar/` directory that no live site owns, and only under `src_dir`.
pub fn prune_stale_sites(src_dir: &Path, sites: &HashSet<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(src_dir) else {
        return;
    };
    if !sites.contains(src_dir) {
        let site_dir = src_dir.join(SITE_DIR);
        if let Ok(files) = std::fs::read_dir(&site_dir) {
            for file in files.flatten() {
                let path = file.path();
                if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                    let _ = std::fs::remove_file(path);
                }
            }
            let _ = std::fs::remove_dir(&site_dir);
        }
    }
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir()
            && !path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with('.'))
        {
            prune_stale_sites(&path, sites);
        }
    }
}
#[cfg(test)]
#[path = "discovery_test.rs"]
mod tests;

/// The `.rsx` that is a directory's module rather than a component in it.
pub const MODULE_ROOT_FILENAME: &str = "mod.rsx";

/// The file a module root's `include!` pulls its children from, written beside the generated module by whoever knows the directory — which the transpiler deliberately does not.
pub const MODULE_CHILDREN_FILENAME: &str = "__children.rs";

/// Whether `path` is a directory's `mod.rsx`. Its stem is `mod`, which [`crate::naming::is_ident`] accepts and rustc does not, so every caller that turns a `.rsx` into a module name has to ask this first.
pub fn is_module_root(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name == MODULE_ROOT_FILENAME)
}

/// The first `//!` or `#![…]` of a file that is about to be `include!`d, which rustc refuses there (`E0753`, and "an inner attribute is not permitted in this context"). Reported before the include is written, so the message names the file and the move instead of landing on generated code.
fn leading_inner_attribute(path: &Path) -> Option<String> {
    let source = std::fs::read_to_string(path).ok()?;
    source
        .lines()
        .map(str::trim_start)
        .find(|line| line.starts_with("//!") || line.starts_with("#!["))
        .map(str::to_owned)
}
