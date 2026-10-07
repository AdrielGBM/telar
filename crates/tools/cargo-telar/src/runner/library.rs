//! `cargo telar package` and `cargo telar publish`: shipping a `[telar] library` together with the artifact a dependency is compiled from.
//!
//! A library compiled as a dependency is wired read-only from the Plain artifact its package carries, and nothing the consumer runs can produce a missing one or refresh a stale one. So the artifact has to be checked where it can still be fixed: here, against the exact files `cargo package` would put in the `.crate`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use telar_project::BuildFlavour;

use super::bake::{bake_workspace, member_dirs};
use super::cli::{PackageArgs, PublishArgs};
use super::config::{find_package_dir, read_package_manifest_in};
use super::transpile::transpile_workspace;

/// A workspace member that declares `library = true`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Library {
    pub(super) name: String,
    pub(super) dir: PathBuf,
}

/// The cargo command a release pipeline ends in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Shipment {
    Package,
    Publish,
}

impl Shipment {
    fn subcommand(self) -> &'static str {
        match self {
            Shipment::Package => "package",
            Shipment::Publish => "publish",
        }
    }
}

pub(crate) fn run_package_cmd(args: PackageArgs) {
    let PackageArgs {
        package,
        workspace,
        check,
        cargo_args,
    } = args;
    ship(
        Shipment::Package,
        package.as_deref(),
        workspace,
        check,
        &cargo_args,
    );
}

pub(crate) fn run_publish_cmd(args: PublishArgs) {
    let PublishArgs {
        package,
        workspace,
        dry_run,
        cargo_args,
    } = args;
    ship(
        Shipment::Publish,
        package.as_deref(),
        workspace,
        false,
        &publish_args(dry_run, cargo_args),
    );
}

pub(super) fn publish_args(dry_run: bool, mut cargo_args: Vec<String>) -> Vec<String> {
    if dry_run && !cargo_args.iter().any(|arg| arg == "--dry-run") {
        cargo_args.insert(0, "--dry-run".to_string());
    }
    cargo_args
}

/// Names of the packages in `metadata` (`cargo metadata --no-deps` output) that `package.publish` forbids publishing: `false` or an empty registry list.
pub(super) fn unpublishable_names(metadata: &str) -> BTreeSet<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(metadata) else {
        return BTreeSet::new();
    };
    value["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|package| package["publish"].as_array().is_some_and(Vec::is_empty))
        .filter_map(|package| package["name"].as_str().map(str::to_string))
        .collect()
}

fn unpublishable_in(workspace_root: &Path) -> BTreeSet<String> {
    Command::new("cargo")
        .args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--manifest-path",
        ])
        .arg(workspace_root.join("Cargo.toml"))
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| unpublishable_names(&String::from_utf8_lossy(&output.stdout)))
        .unwrap_or_default()
}

/// What `cargo publish` can be asked for out of `libraries`: with `--workspace` those that forbid publishing are left out, and a library named with `-p` that forbids it is an error.
pub(super) fn publishable(
    libraries: Vec<Library>,
    unpublishable: &BTreeSet<String>,
    named: bool,
) -> Result<(Vec<Library>, Vec<String>), String> {
    let (kept, skipped): (Vec<Library>, Vec<Library>) = libraries
        .into_iter()
        .partition(|library| !unpublishable.contains(&library.name));
    if named && let Some(library) = skipped.first() {
        return Err(format!(
            "`{}` cannot be published: `package.publish` is false or an empty list in its Cargo.toml",
            library.name
        ));
    }
    Ok((kept, skipped.into_iter().map(|l| l.name).collect()))
}

/// Bakes and transpiles the selected libraries, refuses any that is not ready to ship, then runs `cargo package` or `cargo publish` over all of them at once, so cargo orders them by their dependencies on each other. With `check_only` it stops after the readiness verdict.
fn ship(
    shipment: Shipment,
    package: Option<&str>,
    workspace: bool,
    check_only: bool,
    cargo_args: &[String],
) {
    let here = find_package_dir(&[]);
    let workspace_root = telar_project::find_workspace_root(&here).unwrap_or_else(|| here.clone());
    let libraries = select_libraries(&here, &workspace_root, package, workspace)
        .unwrap_or_else(|message| fail(&message));
    let mut any_skipped = false;
    let libraries = if shipment == Shipment::Publish {
        let (kept, skipped) = publishable(
            libraries,
            &unpublishable_in(&workspace_root),
            package.is_some(),
        )
        .unwrap_or_else(|message| fail(&message));
        if !skipped.is_empty() {
            any_skipped = true;
            eprintln!(
                "[cargo-telar] skipping {} (`package.publish` forbids publishing)",
                skipped.join(", ")
            );
        }
        kept
    } else {
        libraries
    };
    let verb = shipment.subcommand();
    if libraries.is_empty() {
        if any_skipped {
            eprintln!("[cargo-telar] every selected library forbids publishing; nothing to {verb}");
        } else {
            eprintln!(
                "[cargo-telar] no workspace member declares `library = true`; nothing to {verb}"
            );
        }
        return;
    }

    if !bake_workspace() {
        fail(
            "the bake reported errors, so no library can ship an artifact that answers for its sources; fix the errors above and run this again",
        );
    }
    if !transpile_workspace() {
        fail(
            "the transpile failed, so no library can ship an artifact that answers for its sources; fix the errors above and run this again",
        );
    }
    let telar_version = telar_project::resolve_telar_version(&workspace_root)
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());

    let mut ready = true;
    for library in &libraries {
        let problems = readiness(library, &telar_version);
        if problems.is_empty() {
            eprintln!("[cargo-telar] {}: ready to {verb}", library.name);
            continue;
        }
        ready = false;
        eprintln!(
            "[cargo-telar] error: {} is not ready to {verb}:",
            library.name
        );
        for problem in &problems {
            eprintln!("\n{}", indent(problem));
        }
        eprintln!();
    }
    if !ready {
        std::process::exit(1);
    }
    if check_only {
        return;
    }
    run_cargo(shipment, &workspace_root, &libraries, cargo_args);
}

/// The libraries a `cargo telar package` or `publish` invocation names: every library member for `--workspace`, the member called `package`, or else the package `here` is in.
pub(super) fn select_libraries(
    here: &Path,
    workspace_root: &Path,
    package: Option<&str>,
    workspace: bool,
) -> Result<Vec<Library>, String> {
    let members: Vec<Library> = member_dirs(workspace_root)
        .into_iter()
        .filter_map(|dir| {
            let name = read_package_manifest_in(&dir)?.name;
            Some(Library { name, dir })
        })
        .collect();
    if workspace {
        let mut libraries = Vec::new();
        for member in members {
            if is_library(&member.dir)? {
                libraries.push(member);
            }
        }
        return Ok(libraries);
    }
    let selected = match package {
        Some(name) => members
            .into_iter()
            .find(|member| member.name == name)
            .ok_or_else(|| {
                format!(
                    "no package named `{name}` in the workspace at {}",
                    workspace_root.display()
                )
            })?,
        None => {
            let name = read_package_manifest_in(here)
                .ok_or_else(|| {
                    format!(
                        "{} is not a package: run this in a library's directory, or pass `-p <name>` or `--workspace`",
                        here.display()
                    )
                })?
                .name;
            Library {
                name,
                dir: here.to_path_buf(),
            }
        }
    };
    if !is_library(&selected.dir)? {
        return Err(format!(
            "`{}` is not a telar library, so nothing ships a transpiled artifact for it. A package other crates depend on declares it in {}:\n    [telar]\n    library = true",
            selected.name,
            selected
                .dir
                .join(telar_project::MANIFEST_FILENAME)
                .display()
        ));
    }
    Ok(vec![selected])
}

fn is_library(dir: &Path) -> Result<bool, String> {
    telar_project::TelarManifest::load(dir)
        .map(|manifest| manifest.telar.library)
        .map_err(|e| e.to_string())
}

/// Everything that keeps `library` from being packaged as it stands, each phrased with what fixes it. Empty when the package would carry a complete artifact that answers for its sources.
///
/// The artifact is checked in a copy holding exactly the files `cargo package` lists, so what is judged is what would ship, not what happens to be on disk beside it. When the list misses a file, the copy cannot answer anything the missing file would not explain, so the package directory itself is checked instead and both problems are reported.
fn readiness(library: &Library, telar_version: &str) -> Vec<String> {
    let listed = match packaged_files(&library.dir) {
        Ok(listed) => listed,
        Err(message) => return vec![message],
    };
    let mut problems: Vec<String> = missing_from_package(library, &listed).into_iter().collect();
    if !problems.is_empty() {
        problems.extend(artifact_problems(&library.dir, telar_version));
        return problems;
    }
    let staged = std::env::temp_dir().join(format!(
        "cargo-telar-package-{}-{}",
        library.name,
        std::process::id()
    ));
    match stage(&library.dir, &listed, &staged) {
        Ok(()) => problems.extend(artifact_problems(&staged, telar_version)),
        Err(e) => problems.push(format!(
            "could not copy the packaged files to {} to check them: {e}",
            staged.display()
        )),
    }
    let _ = std::fs::remove_dir_all(&staged);
    problems
}

/// The files `cargo package` would put in the package at `dir`, `/`-separated and relative to it.
///
/// `--allow-dirty` because cargo counts a gitignored file it would package as an uncommitted change, and `.telar/` is gitignored in every telar project: the question here is what the package holds, and whether its sources are committed is asked separately before packaging.
pub(super) fn packaged_files(dir: &Path) -> Result<BTreeSet<String>, String> {
    let output = Command::new("cargo")
        .args([
            "package",
            "--list",
            "--allow-dirty",
            "--quiet",
            "--manifest-path",
        ])
        .arg(dir.join("Cargo.toml"))
        .output()
        .map_err(|e| format!("could not run `cargo package --list`: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "`cargo package --list` failed for {}:\n{}",
            dir.display(),
            String::from_utf8_lossy(&output.stderr).trim_end()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| line.trim().replace('\\', "/"))
        .filter(|line| !line.is_empty())
        .collect())
}

/// The problem with `library`'s `include`, when `listed` leaves out a file a build compiling it as a dependency reads, with the entries that would put each one back.
pub(super) fn missing_from_package(library: &Library, listed: &BTreeSet<String>) -> Option<String> {
    let missing: Vec<String> = telar_project::library_files(&library.dir)
        .into_iter()
        .filter(|file| !listed.contains(file))
        .collect();
    if missing.is_empty() {
        return None;
    }
    let manifest = library.dir.join("Cargo.toml");
    let entries: Vec<String> = telar_project::library_include()
        .into_iter()
        .filter(|entry| {
            missing
                .iter()
                .any(|file| telar_project::library_include_covers(entry, file))
        })
        .collect();
    let mut message = format!(
        "`cargo package` would leave out {} file(s) that a build compiling `{}` as a dependency reads:\n{}",
        missing.len(),
        library.name,
        listing(&missing)
    );
    match declared_include(&manifest) {
        Some(_) => message.push_str(&format!(
            "\nAdd to `include` under `[package]` in {}:\n{}",
            manifest.display(),
            entries
                .iter()
                .map(|entry| format!("    \"{entry}\","))
                .collect::<Vec<_>>()
                .join("\n")
        )),
        None => message.push_str(&format!(
            "\n{} declares no `include`, and without one cargo leaves every dot-directory out of a package, `.telar/` among them. Add under `[package]`, keeping any other file the package needs, such as a build script, in the list:\n{}",
            manifest.display(),
            include_block()
        )),
    }
    Some(message)
}

/// The `include` a library's `Cargo.toml` starts with, as written there.
pub(super) fn include_block() -> String {
    let entries: Vec<String> = telar_project::library_include()
        .into_iter()
        .map(|entry| format!("    \"{entry}\","))
        .collect();
    format!("include = [\n{}\n]", entries.join("\n"))
}

fn declared_include(manifest: &Path) -> Option<Vec<String>> {
    let table: toml::Table = std::fs::read_to_string(manifest).ok()?.parse().ok()?;
    let include = table.get("package")?.get("include")?.as_array()?;
    Some(
        include
            .iter()
            .filter_map(|entry| entry.as_str().map(str::to_string))
            .collect(),
    )
}

fn listing(files: &[String]) -> String {
    const SHOWN: usize = 12;
    let mut lines: Vec<String> = files
        .iter()
        .take(SHOWN)
        .map(|file| format!("    {file}"))
        .collect();
    if files.len() > SHOWN {
        lines.push(format!("    … and {} more", files.len() - SHOWN));
    }
    lines.join("\n")
}

fn stage(dir: &Path, listed: &BTreeSet<String>, staged: &Path) -> std::io::Result<()> {
    let _ = std::fs::remove_dir_all(staged);
    std::fs::create_dir_all(staged)?;
    for file in listed {
        let source = dir.join(file);
        if !source.is_file() {
            continue;
        }
        let target = staged.join(file);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(&source, &target)?;
    }
    Ok(())
}

/// Why the Plain artifact in `package_dir` would be refused by a build compiling it as a dependency, against `telar_version`: the same handshakes the macro runs — the build index answering for every `.rsx`, the module tree matching the source tree, and the version each artifact was written for — plus the baked catalog and assets against the sources they were baked from.
pub(super) fn artifact_problems(package_dir: &Path, telar_version: &str) -> Vec<String> {
    let mut problems = Vec::new();
    let prelude = match telar_project::resolve_prelude(package_dir) {
        Ok(prelude) => prelude,
        Err(e) => return vec![e.to_string()],
    };
    let theme = telar_transpiler::resolve_theme_type(package_dir);
    let src_dir = package_dir.join("src");
    let plain = BuildFlavour::Plain;
    let generated_dir = telar_project::generated_dir(package_dir, plain);
    let telar_dir = package_dir.join(".telar");
    let assets = telar_project::AssetContext::load(package_dir, telar_version);
    const RETRANSPILE: &str = "Run `cargo telar transpile` and package again.";

    if !telar_project::find_rsx_files(&src_dir).is_empty() {
        let index_file = format!(".telar/{}", plain.index_filename());
        match telar_project::read_build_index(package_dir, plain) {
            None => problems.push(format!(
                "there is no transpiled artifact: `{index_file}` is missing or unreadable. {RETRANSPILE}"
            )),
            Some(index) if index.format != telar_project::BUILD_ARTIFACT_FORMAT => {
                problems.push(format!(
                    "`{index_file}` is in artifact format {}, and this telar reads format {}. {RETRANSPILE}",
                    index.format,
                    telar_project::BUILD_ARTIFACT_FORMAT
                ))
            }
            Some(index) if index.telar_version != telar_version => problems.push(format!(
                "the `.rsx` were transpiled for telar {}, and this workspace builds telar {telar_version}; a dependency is refused unless the two agree. {RETRANSPILE}",
                index.telar_version
            )),
            Some(index) if !index.was_transpiled_with(&prelude) => problems.push(format!(
                "the `.rsx` were transpiled with `prelude = {:?}`, and telar.toml now declares {:?}. {RETRANSPILE}",
                index.prelude,
                prelude
                    .iter()
                    .map(telar_project::PreludeEntry::path)
                    .collect::<Vec<_>>()
            )),
            Some(index) if index.theme.as_deref() != theme.as_deref() => problems.push(format!(
                "the `.rsx` were transpiled against the theme {:?}, and the package now names {theme:?}. {RETRANSPILE}",
                index.theme
            )),
            Some(index)
                if !index.answers_for(
                    &src_dir,
                    &generated_dir,
                    theme.as_deref(),
                    &prelude,
                    telar_version,
                ) =>
            {
                problems.push(format!(
                    "the transpiled Rust in `.telar/{}/` no longer answers for `src/`: a `.rsx` was added, removed or edited after the transpile, or a generated file was changed. {RETRANSPILE}",
                    plain.dir_name()
                ))
            }
            Some(index) if index.uses_assets && assets.module_file().is_none() => {
                problems.push(format!(
                    "the transpiled Rust reaches into the baked assets, and `.telar/{}` is missing or unusable. Run `cargo telar bake` and package again.",
                    telar_project::ASSETS_SOURCE_FILENAME
                ))
            }
            Some(_) => {}
        }
    }

    if telar_project::invokes_placement_macro(&src_dir)
        && let Some(stale) =
            telar_project::ModuleTree::discover(&src_dir, &generated_dir, plain.site_file_name())
                .first_difference()
    {
        let shown = stale
            .strip_prefix(package_dir)
            .unwrap_or(stale)
            .display()
            .to_string();
        problems.push(format!(
            "the module tree file `{shown}` is missing or out of date: a Rust module was added, moved or removed after the transpile. {RETRANSPILE}"
        ));
    }

    problems.extend(catalog_problems(package_dir, &telar_dir, telar_version));
    problems.extend(asset_problems(package_dir, &telar_dir, telar_version));
    problems.extend(icon_record_problem(package_dir));
    problems
}

/// Why the record of the library's baked icons, which the licence notice of every application built with it is merged from, would not answer for them.
fn icon_record_problem(package_dir: &Path) -> Option<String> {
    telar_baker::read_library_icons(package_dir).err().map(|problem| {
        format!(
            "the icons it baked could not be credited in the licence notice of an application built with it: {problem}. Run `cargo telar bake` and package again."
        )
    })
}

fn catalog_problems(package_dir: &Path, telar_dir: &Path, telar_version: &str) -> Vec<String> {
    const REBAKE: &str = "Run `cargo telar bake` and package again.";
    let index = match telar_project::read_catalog_index(telar_dir) {
        Ok(None) => return Vec::new(),
        Ok(Some(index)) => index,
        Err(e) => {
            return vec![format!(
                "the baked translation catalog cannot be read: {e}. {REBAKE}"
            )];
        }
    };
    if index.format != telar_project::CATALOG_ARTIFACT_FORMAT
        || index.telar_version != telar_version
    {
        return vec![format!(
            "the translation catalog was baked for telar {} in format {}, and this workspace builds telar {telar_version}, which reads format {}. {REBAKE}",
            index.telar_version,
            index.format,
            telar_project::CATALOG_ARTIFACT_FORMAT
        )];
    }
    let catalog = telar_project::CatalogContext::load(package_dir, telar_version);
    if catalog.module_file().is_none() {
        return vec![format!(
            "the translation catalog has an index and no `.telar/{}`. {REBAKE}",
            telar_project::CATALOG_SOURCE_FILENAME
        )];
    }
    catalog.staleness().into_iter().collect()
}

fn asset_problems(package_dir: &Path, telar_dir: &Path, telar_version: &str) -> Vec<String> {
    const REBAKE: &str = "Run `cargo telar bake` and package again.";
    let index = match telar_project::check_artifact(telar_dir, telar_version) {
        Ok(telar_project::ArtifactHandshake::NotBaked) => return Vec::new(),
        Ok(telar_project::ArtifactHandshake::UpToDate(index)) => index,
        Ok(telar_project::ArtifactHandshake::FormatMismatch { found, expected }) => {
            return vec![format!(
                "the baked assets are in format {found}, and this telar reads format {expected}. {REBAKE}"
            )];
        }
        Ok(telar_project::ArtifactHandshake::VersionMismatch { found, expected }) => {
            return vec![format!(
                "the assets were baked for telar {found}, and this workspace builds telar {expected}. {REBAKE}"
            )];
        }
        Err(e) => {
            return vec![format!("the baked assets cannot be read: {e}. {REBAKE}")];
        }
    };
    let mut problems = Vec::new();
    if !telar_dir
        .join(telar_project::ASSETS_SOURCE_FILENAME)
        .is_file()
    {
        problems.push(format!(
            "the baked assets have an index and no `.telar/{}`. {REBAKE}",
            telar_project::ASSETS_SOURCE_FILENAME
        ));
    }
    let root = telar_project::assets_root(package_dir);
    problems.extend(index.entries.iter().filter_map(|entry| {
        let bytes = std::fs::read(root.join(&entry.path)).ok()?;
        (telar_project::content_hash(&bytes) != entry.hash).then(|| {
            format!(
                "the asset `{}` changed after it was baked, so the package would carry its old contents. {REBAKE}",
                entry.path
            )
        })
    }));
    problems
}

/// Runs `cargo package` or `cargo publish` over `libraries`, after refusing sources that are not committed the way cargo itself would.
///
/// Cargo refuses a package holding any file git does not have committed, and the gitignored `.telar/` the package has to carry is exactly that, so it is asked to allow a dirty tree and the check is made here instead, over every packaged source git does not ignore.
fn run_cargo(
    shipment: Shipment,
    workspace_root: &Path,
    libraries: &[Library],
    cargo_args: &[String],
) {
    let verb = shipment.subcommand();
    if !cargo_args.iter().any(|arg| arg == "--allow-dirty") {
        for library in libraries {
            let listed = packaged_files(&library.dir).unwrap_or_else(|message| fail(&message));
            let dirty = uncommitted(&library.dir, &listed);
            if !dirty.is_empty() {
                fail(&format!(
                    "{} file(s) of `{}` have changes not yet committed to git:\n{}\nCommit them, or {verb} them anyway with `cargo telar {verb} -p {} -- --allow-dirty`.",
                    dirty.len(),
                    library.name,
                    listing(&dirty),
                    library.name
                ));
            }
        }
    }
    let names: Vec<&str> = libraries.iter().map(|l| l.name.as_str()).collect();
    eprintln!("[cargo-telar] cargo {verb} -p {}", names.join(" -p "));
    let status = Command::new("cargo")
        .args(cargo_invocation(
            shipment,
            workspace_root,
            libraries,
            cargo_args,
        ))
        .status();
    match status {
        Ok(status) if status.success() => {}
        Ok(status) => std::process::exit(status.code().unwrap_or(1)),
        Err(e) => fail(&format!("could not run `cargo {verb}`: {e}")),
    }
}

/// The arguments of the cargo call that ships `libraries`: always `--allow-dirty`, since the committed-sources check has already been made, then the caller's own arguments.
pub(super) fn cargo_invocation(
    shipment: Shipment,
    workspace_root: &Path,
    libraries: &[Library],
    cargo_args: &[String],
) -> Vec<String> {
    let mut args = vec![
        shipment.subcommand().to_string(),
        "--allow-dirty".to_string(),
        "--manifest-path".to_string(),
        workspace_root.join("Cargo.toml").display().to_string(),
    ];
    for library in libraries {
        args.push("-p".to_string());
        args.push(library.name.clone());
    }
    args.extend(
        cargo_args
            .iter()
            .filter(|arg| *arg != "--allow-dirty")
            .cloned(),
    );
    args
}

/// The packaged sources of the package at `dir` that git reports as modified or untracked. Empty outside a git repository, where cargo has nothing to compare against either.
fn uncommitted(dir: &Path, listed: &BTreeSet<String>) -> Vec<String> {
    let git = |args: &[&str]| {
        Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
    };
    let Some(prefix) = git(&["rev-parse", "--show-prefix"]) else {
        return Vec::new();
    };
    let Some(status) = git(&[
        "status",
        "--porcelain",
        "-z",
        "--untracked-files=all",
        "--",
        ".",
    ]) else {
        return Vec::new();
    };
    uncommitted_sources(&status, prefix.trim(), listed)
}

/// The packaged sources `status` (`git status --porcelain -z` run in the package, whose path from the repository root is `prefix`) reports as changed. The artifact under `.telar/` is regenerated by every transpile and checked for freshness separately, so a library that commits it is not held to the committed copy matching the fresh one.
pub(super) fn uncommitted_sources(
    status: &str,
    prefix: &str,
    listed: &BTreeSet<String>,
) -> Vec<String> {
    changed_paths(status)
        .into_iter()
        .filter_map(|path| path.strip_prefix(prefix))
        .filter(|path| !path.starts_with(".telar/"))
        .filter(|path| listed.contains(*path))
        .map(str::to_string)
        .collect()
}

/// The paths of `git status --porcelain -z`, relative to the repository root. A rename or copy is followed by the path it came from, which is not a change of its own.
fn changed_paths(status: &str) -> Vec<&str> {
    let mut paths = Vec::new();
    let mut entries = status.split('\0').filter(|entry| !entry.is_empty());
    while let Some(entry) = entries.next() {
        let Some(path) = entry.get(3..) else {
            continue;
        };
        paths.push(path);
        if entry.starts_with(['R', 'C']) {
            entries.next();
        }
    }
    paths
}

fn indent(text: &str) -> String {
    text.lines()
        .map(|line| format!("  {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn fail(message: &str) -> ! {
    eprintln!("[cargo-telar] error: {message}");
    std::process::exit(1);
}

#[cfg(test)]
#[path = "library_test.rs"]
mod tests;
