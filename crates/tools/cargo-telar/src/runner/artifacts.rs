//! Running a cargo build and learning from its JSON messages where it put each artifact.
//!
//! Cargo already decided the profile directory, the target directory and the file names (`[lib] name`, `[[bin]] name`, `--profile`, `--target`, whatever came after `--`), and its `compiler-artifact` messages say what it decided. Rebuilding any of those paths here would be a second opinion that is wrong whenever a flag or a manifest key it does not know about is in play.

use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::Deserialize;

use super::diagnostics;

/// One `compiler-artifact` message: a target cargo built, and the files it left.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub(crate) struct Artifact {
    manifest_path: PathBuf,
    target: ArtifactTarget,
    filenames: Vec<PathBuf>,
    executable: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct ArtifactTarget {
    name: String,
    kind: Vec<String>,
}

impl Artifact {
    /// The artifact a JSON message reports, or `None` for any other message.
    pub(crate) fn from_message(message: &serde_json::Value) -> Option<Self> {
        if message.get("reason")?.as_str()? != "compiler-artifact" {
            return None;
        }
        Self::deserialize(message).ok()
    }

    fn belongs_to(&self, package_dir: &Path) -> bool {
        same_file(&self.manifest_path, &package_dir.join("Cargo.toml"))
    }

    fn is(&self, kind: &str) -> bool {
        self.target.kind.iter().any(|k| k == kind)
    }
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// The `cdylib` file with `extension` that the build reported for the package in `package_dir`: the host's dynamic library ([`std::env::consts::DLL_EXTENSION`]) or a browser module (`wasm`).
pub(crate) fn cdylib(
    artifacts: &[Artifact],
    package_dir: &Path,
    extension: &str,
) -> Option<PathBuf> {
    artifacts
        .iter()
        .filter(|artifact| artifact.is("cdylib") && artifact.belongs_to(package_dir))
        .flat_map(|artifact| &artifact.filenames)
        .find(|file| file.extension().is_some_and(|ext| ext == extension))
        .cloned()
}

/// The binary the build reported for the package in `package_dir`, chosen the way `cargo run` chooses: the only one built, or else the one `default_run` names.
pub(crate) fn binary(
    artifacts: &[Artifact],
    package_dir: &Path,
    default_run: Option<&str>,
) -> Result<PathBuf, String> {
    let built: Vec<(&str, &Path)> = artifacts
        .iter()
        .filter(|artifact| artifact.is("bin") && artifact.belongs_to(package_dir))
        .filter_map(|artifact| {
            Some((
                artifact.target.name.as_str(),
                artifact.executable.as_deref()?,
            ))
        })
        .collect();
    if let [(_, only)] = built.as_slice() {
        return Ok(only.to_path_buf());
    }
    if let Some((_, chosen)) = built.iter().find(|(name, _)| Some(*name) == default_run) {
        return Ok(chosen.to_path_buf());
    }
    match built.is_empty() {
        true => Err(format!(
            "cargo reported no binary for the package in {}. Does it have a `[[bin]]`?",
            package_dir.display()
        )),
        false => {
            let names: Vec<String> = built.iter().map(|(name, _)| format!("`{name}`")).collect();
            Err(format!(
                "the package in {} builds several binaries ({}); name the one to run with `default-run` under its `[package]`.",
                package_dir.display(),
                names.join(", ")
            ))
        }
    }
}

/// Adds `--message-format=json` unless the caller already chose a format, which cargo would reject as two of them rather than honour the later one.
pub(crate) fn with_json_messages(args: &mut Vec<String>) {
    if !args.iter().any(|a| a.starts_with("--message-format")) {
        args.push("--message-format=json".to_string());
    }
}

/// Runs a cargo build and re-points whatever it says about generated Rust back onto the `.rsx` it came from; the report also carries the artifacts cargo reported.
///
/// stdout carries the JSON message stream and is consumed here; stderr is cargo's own progress and is left on the terminal, so a build still looks like a build. The report is printed here, warnings included: they used to be captured into a `String` that was read on the failure path alone, which made them invisible for a whole development session.
pub(crate) fn build_with_diagnostics(cmd: &mut Command) -> (bool, diagnostics::Report) {
    cmd.arg("--color=always")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    let mut child = cmd.spawn().expect("[cargo-telar] failed to invoke cargo");
    // Drained before `wait`, or a report larger than the pipe buffer deadlocks the build it is reading.
    let report = match child.stdout.take() {
        Some(stdout) => diagnostics::collect(BufReader::new(stdout)),
        None => diagnostics::Report::default(),
    };
    let succeeded = child.wait().map(|status| status.success()).unwrap_or(false);
    if report.has_diagnostics() {
        eprintln!();
        eprint!("{}", report.render(true));
    }
    (succeeded, report)
}

#[cfg(test)]
#[path = "artifacts.test.rs"]
mod tests;
