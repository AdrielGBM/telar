//! The `[logic]` zone: statement-level Rust wrapped into a synthetic item so `rustfmt` will take it, then unwrapped again.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::INDENT;

/// Synthetic wrapper used to make the statement-level `[logic]` zone a valid Rust item for `rustfmt`.
pub(super) const WRAPPER_FN: &str = "__rsx_fmt_logic_wrapper";

pub(super) fn format_logic_section(logic: &str) -> String {
    let body = run_rustfmt_on_logic(logic).unwrap_or_else(|| logic.trim_end().to_string());
    let body = body.trim_end();
    if body.is_empty() {
        "[logic]".to_string()
    } else {
        format!("[logic]\n{body}")
    }
}

/// Reformats the logic zone with `rustfmt`. Returns `None` (so the caller keeps the source verbatim) when `rustfmt` is missing or rejects the input.
pub(super) fn run_rustfmt_on_logic(logic: &str) -> Option<String> {
    let logic = logic.trim_end();
    if logic.trim().is_empty() {
        return None;
    }

    let wrapped = format!("fn {WRAPPER_FN}() {{\n{logic}\n}}\n");
    let formatted = run_rustfmt(&wrapped)?;
    unwrap_logic(&formatted)
}

/// Strips the synthetic wrapper function and one level of indentation that `rustfmt` added, and turns preview sentinel comments back into attributes.
pub(super) fn unwrap_logic(formatted: &str) -> Option<String> {
    let lines: Vec<&str> = formatted.lines().collect();
    let first = lines.first()?;
    if !first.trim_start().starts_with(&format!("fn {WRAPPER_FN}")) {
        return None;
    }
    // The wrapper's closing brace is the last non-blank line.
    let close = lines.iter().rposition(|l| l.trim() == "}")?;
    if close == 0 {
        return None;
    }

    // rustfmt exits 0 having only partly reformatted a body it could not fully parse: the inline prop-default sugar is not valid Rust, so a `Props` struct using it comes back with the wrapper's indent on the lines around it and none on its own fields. There is then no right amount to strip, so keep the source verbatim.
    let body = &lines[1..close];
    if !body
        .iter()
        .all(|line| line.trim().is_empty() || line.starts_with(INDENT))
    {
        return None;
    }
    let body: Vec<String> = body
        .iter()
        .map(|line| line.strip_prefix(INDENT).unwrap_or(line).to_string())
        .collect();

    Some(body.join("\n").trim_end().to_string())
}

pub(super) fn run_rustfmt(input: &str) -> Option<String> {
    let rustfmt = find_rustfmt()?;
    let mut child = Command::new(rustfmt)
        .arg("--edition")
        .arg("2024")
        .arg("--emit")
        .arg("stdout")
        .arg("--quiet")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    {
        let mut stdin = child.stdin.take()?;
        stdin.write_all(input.as_bytes()).ok()?;
    }

    let output = child.wait_with_output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

pub(super) fn find_rustfmt() -> Option<PathBuf> {
    let exe = format!("rustfmt{}", std::env::consts::EXE_SUFFIX);

    let path_env = std::env::var("PATH").unwrap_or_default();
    let sep = if cfg!(windows) { ';' } else { ':' };
    for dir in path_env.split(sep) {
        let candidate = Path::new(dir).join(&exe);
        if candidate.exists() {
            return Some(candidate);
        }
    }

    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()?;
    let cargo_bin = Path::new(&home).join(".cargo").join("bin").join(&exe);
    cargo_bin.exists().then_some(cargo_bin)
}
