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

    let report = match child.stdout.take() {
        Some(stdout) => diagnostics::collect(BufReader::new(stdout)),
        None => diagnostics::Report::default(),
    };
    let status = child.wait();

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
