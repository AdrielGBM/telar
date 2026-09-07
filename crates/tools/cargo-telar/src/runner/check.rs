//! `cargo telar check` — cargo's own diagnostics, re-pointed at the `.rsx` that produced them.
//!
//! The mapping itself lives in [`super::diagnostics`], which the `cargo telar dev` rebuild loop shares.

use std::io::BufReader;
use std::process::{Command, Stdio};

use super::cli::CheckArgs;
use super::diagnostics;

pub(crate) fn run_check_cmd(args: CheckArgs) {
    let mut cmd = Command::new("cargo");
    cmd.arg("check")
        .arg("--message-format=json")
        .arg("--color=always");
    if let Some(package) = &args.common.package {
        cmd.arg("-p").arg(package);
    }
    if let Some(features) = &args.common.features {
        cmd.arg("--features").arg(features);
    }
    // A `[preview]` is markup the author wrote, so a check that skipped it would report nothing about the one block most likely to be half-finished. It is the only command that asks for previews without going on to render them.
    cmd.arg("--features").arg("telar/previews");
    if args.all_targets {
        cmd.arg("--all-targets");
    }
    cmd.args(&args.common.cargo_args);
    // cargo writes its JSON stream to stdout and its human progress to stderr; letting stderr through keeps the familiar "Checking foo v0.1.0" output while the machine-readable half is consumed here.
    cmd.stdout(Stdio::piped()).stderr(Stdio::inherit());

    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(e) => {
            eprintln!("[cargo-telar] could not run cargo check: {e}");
            std::process::exit(1);
        }
    };

    let mut report = match child.stdout.take() {
        Some(stdout) => diagnostics::collect(BufReader::new(stdout)),
        None => diagnostics::Report::default(),
    };
    let status = child.wait();

    report.add_all_semantic();

    if !report.is_empty() {
        eprintln!();
        eprint!("{}", report.render(true));
    }

    let code = status
        .ok()
        .and_then(|status| status.code())
        .unwrap_or(if report.has_errors() { 1 } else { 0 });
    std::process::exit(code);
}

impl diagnostics::Report {
    /// The `.rsx` checks the language server runs, over every source in the workspace.
    ///
    /// Here rather than only in the editor because the two disagreed: an undeclared style class or an i18n key the catalogue does not hold was underlined in VS Code and compiled clean from a terminal, so a project could pass its own check and still be wrong on the screen of whoever opened it.
    ///
    /// They are markup checks, not type checks — cargo answers for the Rust — so they run whether or not cargo found anything.
    fn add_all_semantic(&mut self) {
        let dir = super::config::find_package_dir(&[]);
        let root = telar_project::find_workspace_root(&dir).unwrap_or(dir);
        for member in super::bake::member_dirs(&root) {
            let keys = catalog_keys(&member);
            let catalog =
                (!keys.is_empty()).then(|| telar_diagnostics::CatalogView { keys: &keys });
            for rsx in telar_project::find_rsx_files(&member.join("src")) {
                let Ok(source) = std::fs::read_to_string(&rsx) else {
                    continue;
                };
                // A file that does not parse is reported by the macro, on the line it broke, with the text the parser produced. Saying it twice from two processes only makes the second one look like a different problem.
                let Ok(document) = telar_parser::parse(&source) else {
                    continue;
                };
                self.add_semantic(
                    &rsx,
                    telar_diagnostics::semantic_diagnostics(&document, catalog.as_ref()),
                );
            }
        }
    }
}

/// Every key one package's catalog defines, or empty when it has none. A malformed catalog reads as empty: the bake reports that, and refusing to run the other checks over it would hide a class error behind a translation one.
fn catalog_keys(package_root: &std::path::Path) -> std::collections::HashSet<String> {
    telar_baker::parse_catalog(package_root)
        .ok()
        .flatten()
        .map(|c| c.keys().cloned().collect())
        .unwrap_or_default()
}
