use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const EDITION: &str = "2024";

/// The environment variables that decide which `rustfmt` runs, read once so resolution can be tested on any values.
#[derive(Debug, Default)]
pub(super) struct RustfmtEnv {
    pub rustfmt: Option<OsString>,
    pub path: Option<OsString>,
    pub cargo_home: Option<OsString>,
    pub home: Option<OsString>,
}

impl RustfmtEnv {
    fn from_process() -> Self {
        Self {
            rustfmt: std::env::var_os("RUSTFMT"),
            path: std::env::var_os("PATH"),
            cargo_home: std::env::var_os("CARGO_HOME"),
            home: std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")),
        }
    }
}

/// Picks `rustfmt` the way `cargo fmt` does: `RUSTFMT` verbatim when set, else the first `PATH` entry holding it, else `$CARGO_HOME/bin` (`~/.cargo/bin` when unset).
pub(super) fn resolve_rustfmt(env: &RustfmtEnv) -> Option<PathBuf> {
    if let Some(explicit) = env.rustfmt.as_ref().filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(explicit));
    }
    let exe = format!("rustfmt{}", std::env::consts::EXE_SUFFIX);
    let on_path = env
        .path
        .iter()
        .flat_map(std::env::split_paths)
        .map(|dir| dir.join(&exe))
        .find(|candidate| candidate.is_file());
    on_path.or_else(|| {
        let cargo_home = env
            .cargo_home
            .as_ref()
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .or_else(|| env.home.as_ref().map(|home| Path::new(home).join(".cargo")))?;
        let candidate = cargo_home.join("bin").join(&exe);
        candidate.is_file().then_some(candidate)
    })
}

pub(super) fn find_rustfmt() -> Option<PathBuf> {
    resolve_rustfmt(&RustfmtEnv::from_process())
}

/// Formats Rust source through `rustfmt` over stdin, so nothing is written and a rejected input leaves the original untouched. The error is `rustfmt`'s own message.
pub fn rustfmt_stdin(source: &str) -> Result<String, String> {
    let rustfmt = find_rustfmt().ok_or("could not find rustfmt")?;
    let mut child = Command::new(rustfmt)
        .args(["--edition", EDITION, "--emit", "stdout", "--quiet"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not run rustfmt: {e}"))?;
    child
        .stdin
        .take()
        .ok_or("rustfmt took no stdin")?
        .write_all(source.as_bytes())
        .map_err(|e| e.to_string())?;
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    String::from_utf8(out.stdout).map_err(|e| e.to_string())
}

#[cfg(test)]
#[path = "rustfmt.test.rs"]
mod tests;
