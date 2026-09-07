//! `cargo telar transpile` and the single point that runs it before every subcommand that goes on to compile.
//!
//! The same discipline as the bake next door, and downstream of it: a `src:"…"` resolves against the baked artifact, so transpiling first would freeze whatever the last bake left. Every route in this binary that spawns `cargo` bakes and then transpiles, in that order.
//!
//! Both flavours are written on every run. Which one a build will read is not known here — `cargo telar dev` picks it, a plain `cargo build` after it picks the other — and producing one is a parse and a codegen over files that are already in the page cache. Guessing wrong would be worse than free: the macro would find an index that does not answer for the sources and transpile the package itself, silently paying for both.
//!
//! **Nothing is pruned here.** The generated directory also holds the module tree the macro writes, and a sweep that knows only about `.rsx` output would delete it. The macro has the whole picture and keeps that job.

use std::path::Path;

use telar_transpiler::{BuildFlavour, PackageOptions};

use super::bake::member_dirs;
use super::config::find_package_dir;

/// Transpiles every workspace member's `.rsx`, in both build flavours.
pub(crate) fn transpile_workspace() {
    let dir = find_package_dir(&[]);
    let workspace_root = telar_transpiler::find_workspace_root(&dir).unwrap_or_else(|| dir.clone());
    let telar_version = telar_transpiler::resolve_telar_version(&workspace_root)
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());
    let producer = format!("cargo-telar {}", env!("CARGO_PKG_VERSION"));

    for member in member_dirs(&workspace_root) {
        transpile_member(&member, &producer, &telar_version);
    }
}

fn transpile_member(member: &Path, producer: &str, telar_version: &str) {
    let src_dir = member.join("src");
    if telar_transpiler::find_rsx_files(&src_dir).is_empty() {
        return;
    }
    let name = member
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("package");
    // Loaded, never baked: the bake already ran, and a `src:"…"` the artifact cannot answer for is an error the macro puts on its own `.rsx` line rather than one this pass can act on.
    let assets = telar_transpiler::AssetContext::load(member, telar_version);
    let theme = telar_transpiler::resolve_theme_type(member);

    for flavour in [BuildFlavour::Plain, BuildFlavour::Hot] {
        let files = match telar_transpiler::transpile_package(&PackageOptions {
            src_dir: &src_dir,
            theme_type: theme.as_deref(),
            assets: Some(&assets),
            flavour,
        }) {
            Ok(files) => files,
            // Reported by the compiler, on the `.rsx` line, once the macro reaches the same file — saying it twice from two processes only makes the second one look like a different problem.
            Err(_) => return,
        };
        let generated_dir = telar_transpiler::generated_dir(member, flavour);
        if let Err(e) = telar_transpiler::write_package(&files, &generated_dir) {
            eprintln!("[cargo-telar] warning: could not write {name}'s generated Rust: {e}");
            return;
        }
        let index = telar_transpiler::build_index(
            &files,
            &src_dir,
            theme.as_deref(),
            producer,
            telar_version,
        );
        if let Err(e) = telar_transpiler::write_build_index(member, flavour, &index) {
            eprintln!("[cargo-telar] warning: could not write {name}'s build index: {e}");
        }
    }
}
