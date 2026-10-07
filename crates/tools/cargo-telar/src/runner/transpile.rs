//! `cargo telar transpile` and the single point that runs it before every subcommand that goes on to compile.
//!
//! The same discipline as the bake next door, and downstream of it: a `src:"…"` resolves against the baked artifact, so transpiling first would freeze whatever the last bake left. Every route in this binary that spawns `cargo` bakes and then transpiles, in that order.
//!
//! Every flavour is written on every run. Which one a build will read is not known here — `cargo telar dev` picks one, a plain `cargo build` after it picks another, `cargo telar test` a third — and producing one is a parse and a codegen over files that are already in the page cache. Guessing wrong would be worse than free: the macro would find an index that does not answer for the sources and transpile the package itself, silently paying for both.
//!
//! **Nothing is pruned here.** The generated directory also holds the module tree the macro writes, and a sweep that knows only about `.rsx` output would delete it. The macro has the whole picture and keeps that job.

use std::path::Path;

use telar_project::BuildFlavour;
use telar_transpiler::PackageOptions;

use super::bake::member_dirs;
use super::config::find_package_dir;

/// Transpiles every workspace member's `.rsx`, in both build flavours, and says whether every one of them transpiled.
///
/// A failure is reported here and recorded for the macro (see [`telar_project::BuildFailure`]) rather than ending the process: the watch loops call this between rebuilds and must survive a typo, and the build that follows reports the same error through the compiler.
pub(crate) fn transpile_workspace() -> bool {
    let dir = find_package_dir(&[]);
    let workspace_root = telar_project::find_workspace_root(&dir).unwrap_or_else(|| dir.clone());
    let telar_version = telar_project::resolve_telar_version(&workspace_root)
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());
    super::config::warn_if_foreign_version(&telar_version);
    let producer = format!("cargo-telar {}", env!("CARGO_PKG_VERSION"));

    // Collected before judging, so a failing member does not stop the ones after it from being transpiled and reported.
    member_dirs(&workspace_root)
        .iter()
        .map(|member| transpile_member(member, &producer, &telar_version))
        .collect::<Vec<bool>>()
        .iter()
        .all(|ok| *ok)
}

/// Transpiles one member in every flavour. `false` when a `.rsx` was refused or the output could not be written; a package with no markup is trivially fine.
pub(super) fn transpile_member(member: &Path, producer: &str, telar_version: &str) -> bool {
    let src_dir = member.join("src");
    if telar_project::find_rsx_files(&src_dir).is_empty() {
        return true;
    }
    let name = member
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("package");
    // Loaded, never baked: the bake already ran, and a `src:"…"` the artifact cannot answer for is an error the macro puts on its own `.rsx` line rather than one this pass can act on.
    let assets = telar_project::AssetContext::load(member, telar_version);
    let theme = telar_transpiler::resolve_theme_type(member);
    let prelude = match telar_project::resolve_prelude(member) {
        Ok(prelude) => prelude,
        Err(e) => {
            eprintln!("[cargo-telar] error: {e}");
            return false;
        }
    };

    let mut reported: Vec<String> = Vec::new();
    let mut all_ok = true;
    for flavour in BuildFlavour::ALL {
        let files = match telar_transpiler::transpile_package(&PackageOptions {
            src_dir: &src_dir,
            theme_type: theme.as_deref(),
            assets: Some(&assets),
            prelude: &prelude,
            flavour,
        }) {
            Ok(files) => files,
            Err(error) => {
                all_ok = false;
                let failure = error.to_build_failure(&src_dir);
                if !reported.contains(&failure.message) {
                    eprintln!("[cargo-telar] error: {}", failure.message);
                    reported.push(failure.message.clone());
                }
                if let Err(e) = telar_project::write_build_failure(member, flavour, &failure) {
                    eprintln!(
                        "[cargo-telar] warning: could not record {name}'s transpile error: {e}"
                    );
                }
                continue;
            }
        };
        telar_project::clear_build_failure(member, flavour);
        let generated_dir = telar_project::generated_dir(member, flavour);
        if let Err(e) = telar_transpiler::write_package(&files, &generated_dir) {
            eprintln!("[cargo-telar] warning: could not write {name}'s generated Rust: {e}");
            return false;
        }
        let index = telar_transpiler::build_index(
            &files,
            &src_dir,
            theme.as_deref(),
            &prelude,
            producer,
            telar_version,
        );
        if let Err(e) = telar_project::write_build_index(member, flavour, &index) {
            eprintln!("[cargo-telar] warning: could not write {name}'s build index: {e}");
            all_ok = false;
        }
    }
    all_ok
}

#[cfg(test)]
#[path = "transpile_test.rs"]
mod tests;
