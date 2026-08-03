//! `cargo telar check` — cargo's own diagnostics, re-pointed at the `.rsx` that produced them.
//!
//! A `.rsx` compiles to `<crate>/.telar/build/<rel>.rs`, so every rustc error past the parse stage names a file
//! the author never wrote and a line they never typed. The transpiler already writes a per-line source map
//! beside each generated file, but until now only the VS Code extension read it — which left every terminal, every
//! CI log, and every other editor pointing into `.telar/`.
//!
//! Parse errors do not need this: the macro reports them as `compile_error!("<file>:<line>: …")`, so the text
//! already names the `.rsx`. It is the errors *after* transpiling — a wrong type, an unresolved name, a
//! component called with the wrong arity — that lose their origin, and those are exactly the ones a person
//! writing `.rsx` hits most.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};

use super::cli::CheckArgs;

/// A rustc diagnostic that started life in a `.rsx`.
struct Projected {
    source: PathBuf,
    /// 1-based, for display.
    line: usize,
    level: String,
    message: String,
}

pub(crate) fn run_check_cmd(args: CheckArgs) {
    let mut cmd = Command::new("cargo");
    cmd.arg("check").arg("--message-format=json");
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

    let mut maps: HashMap<PathBuf, Option<Vec<Option<usize>>>> = HashMap::new();
    let mut projected: Vec<Projected> = Vec::new();
    let mut rendered_passthrough: Vec<String> = Vec::new();

    if let Some(stdout) = child.stdout.take() {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            if value.get("reason").and_then(serde_json::Value::as_str) != Some("compiler-message") {
                continue;
            }
            let Some(message) = value.get("message") else {
                continue;
            };
            let level = message
                .get("level")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("error");
            if level != "error" && level != "warning" {
                continue;
            }
            let text = message
                .get("message")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string();
            let rendered = message
                .get("rendered")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string();

            let mut mapped_any = false;
            for span in primary_spans(message) {
                let Some(file) = span.get("file_name").and_then(serde_json::Value::as_str) else {
                    continue;
                };
                let generated = PathBuf::from(file);
                if !is_generated(&generated) {
                    continue;
                }
                let Some(line_start) = span.get("line_start").and_then(serde_json::Value::as_u64)
                else {
                    continue;
                };
                let Some(source) = generated_to_source(&generated) else {
                    continue;
                };
                let map = maps
                    .entry(generated.clone())
                    .or_insert_with(|| read_line_map(&generated));
                let Some(map) = map.as_ref() else { continue };
                // rustc counts from 1, the map from 0.
                let Some(Some(rsx_line)) = map.get(line_start as usize - 1) else {
                    continue;
                };
                projected.push(Projected {
                    source,
                    line: rsx_line + 1,
                    level: level.to_string(),
                    message: text.clone(),
                });
                mapped_any = true;
            }
            if !mapped_any && !rendered.is_empty() {
                rendered_passthrough.push(rendered);
            }
        }
    }

    let status = child.wait();

    // The `.rsx` view first: it is the file the author can act on, and burying it under cargo's own rendering of generated code is what this command exists to stop.
    if !projected.is_empty() {
        eprintln!();
        eprintln!("[cargo-telar] diagnostics mapped back to their `.rsx` source:");
        for item in &projected {
            eprintln!(
                "  {}: {}:{}: {}",
                item.level,
                display(&item.source),
                item.line,
                item.message
            );
        }
        eprintln!();
    }
    for rendered in &rendered_passthrough {
        eprint!("{rendered}");
    }

    let code = status
        .ok()
        .and_then(|status| status.code())
        .unwrap_or(if projected.is_empty() { 0 } else { 1 });
    std::process::exit(code);
}

fn primary_spans(message: &serde_json::Value) -> Vec<&serde_json::Value> {
    message
        .get("spans")
        .and_then(serde_json::Value::as_array)
        .map(|spans| {
            spans
                .iter()
                .filter(|span| {
                    span.get("is_primary")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `<crate>/.telar/build/<rel>.rs` — or `build-hot`, which a hot-reload build writes instead.
fn is_generated(path: &Path) -> bool {
    if path.extension().and_then(|e| e.to_str()) != Some("rs") {
        return false;
    }
    build_root(path).is_some()
}

/// The index of the `.telar` component and the build-dir name that follows it.
fn build_root(path: &Path) -> Option<usize> {
    let parts: Vec<Component> = path.components().collect();
    parts.iter().enumerate().position(|(i, part)| {
        part.as_os_str() == ".telar"
            && matches!(
                parts.get(i + 1).map(|part| part.as_os_str()),
                Some(next) if next == "build" || next == "build-hot"
            )
    })
}

/// Maps a generated `<crate>/.telar/build/<rel>.rs` back to `<crate>/src/<rel>.rsx`.
fn generated_to_source(generated: &Path) -> Option<PathBuf> {
    let at = build_root(generated)?;
    let parts: Vec<Component> = generated.components().collect();
    let mut source: PathBuf = parts[..at].iter().collect();
    source.push("src");
    for part in &parts[at + 2..] {
        source.push(part.as_os_str());
    }
    Some(source.with_extension("rsx"))
}

/// The sibling `.rs.map`: generated line (0-based index) to `.rsx` line (0-based value).
fn read_line_map(generated: &Path) -> Option<Vec<Option<usize>>> {
    let mut path = generated.as_os_str().to_os_string();
    path.push(".map");
    let text = std::fs::read_to_string(PathBuf::from(path)).ok()?;
    serde_json::from_str(&text).ok()
}

fn display(path: &Path) -> String {
    std::env::current_dir()
        .ok()
        .and_then(|cwd| path.strip_prefix(cwd).ok().map(Path::to_path_buf))
        .unwrap_or_else(|| path.to_path_buf())
        .display()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_generated_file_maps_back_to_its_rsx() {
        let generated = PathBuf::from("/w/crates/modules/.telar/build/clock/clock.rs");
        assert!(is_generated(&generated));
        assert_eq!(
            generated_to_source(&generated),
            Some(PathBuf::from("/w/crates/modules/src/clock/clock.rsx"))
        );
    }

    /// A hot-reload build writes to `build-hot`, and its diagnostics need the same treatment.
    #[test]
    fn a_hot_reload_build_dir_maps_too() {
        let generated = PathBuf::from("/w/apps/a/.telar/build-hot/home.rs");
        assert!(is_generated(&generated));
        assert_eq!(
            generated_to_source(&generated),
            Some(PathBuf::from("/w/apps/a/src/home.rsx"))
        );
    }

    #[test]
    fn a_hand_written_rust_file_is_left_alone() {
        assert!(!is_generated(Path::new("/w/crates/ui/src/icon/mod.rs")));
        assert!(!is_generated(Path::new("/w/crates/ui/.telar/other/x.rs")));
    }
}
