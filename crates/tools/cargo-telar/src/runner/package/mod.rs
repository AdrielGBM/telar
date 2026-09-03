use std::path::{Path, PathBuf};
use std::process::Command;

use super::config::{
    ResolvedPackage, TelarConfig, backend_as_str, resolve_package, split_android_flag,
};

mod appimage;
mod deb;
mod dmg;
mod nsis;
mod web;

pub(crate) use appimage::build_appimage;
pub(crate) use deb::build_deb;
pub(crate) use dmg::build_dmg;
pub(crate) use nsis::build_nsis;
pub(crate) use web::{build_web, build_web_bundle};

pub(crate) fn package_lib_path(
    workspace_root: &Path,
    package_name: &str,
    profile: &str,
) -> PathBuf {
    let lib_name = package_name.replace('-', "_");
    #[cfg(target_os = "macos")]
    let file = format!("lib{lib_name}.dylib");
    #[cfg(target_os = "windows")]
    let file = format!("{lib_name}.dll");
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let file = format!("lib{lib_name}.so");
    workspace_root.join("target").join(profile).join(file)
}

pub(crate) fn package_bin_path(
    workspace_root: &Path,
    package_name: &str,
    profile: &str,
) -> PathBuf {
    // EXE_SUFFIX so `dir` packaging and the hot-reload spawn find `<name>.exe` on Windows.
    workspace_root
        .join("target")
        .join(profile)
        .join(format!("{package_name}{}", std::env::consts::EXE_SUFFIX))
}

// Distribution bundles live under target/ (a build artifact next to the binary they package), so they inherit target's gitignore and are never confused with .rsx/ (which is generated source).
pub(crate) fn dist_dir(workspace_root: &Path) -> PathBuf {
    workspace_root.join("target").join("telar-dist")
}

// The release/debug profile cargo emits into, mirroring build_cargo_args's `--release` handling.
pub(crate) fn profile_of(args: &[String]) -> &'static str {
    if args.contains(&"--release".to_string()) {
        "release"
    } else {
        "debug"
    }
}

// Runs a packaging tool as the terminal step: reports success, forwards the tool's exit code, or exits 1 with an optional install hint when the binary is missing. Diverges since every bundler ends here.
fn run_bundler_tool(
    cmd: &mut Command,
    success_label: &str,
    packaged_at: &Path,
    missing_hint: Option<&str>,
) -> ! {
    match cmd.status() {
        Ok(status) if status.success() => {
            eprintln!(
                "[cargo-telar] Packaged {success_label} at {}",
                packaged_at.display()
            );
            std::process::exit(0);
        }
        Ok(status) => std::process::exit(status.code().unwrap_or(1)),
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound
                && let Some(hint) = missing_hint
            {
                eprintln!("{hint}");
            } else {
                eprintln!(
                    "[cargo-telar] Failed to invoke {}: {e}",
                    cmd.get_program().to_string_lossy()
                );
            }
            std::process::exit(1);
        }
    }
}

// Runs the shared release build for the desktop packaging formats (dir/deb/appimage/dmg/nsis) and returns the built binary path alongside the resolved package (workspace root plus manifest fields the bundlers read).
fn run_release_build(cargo_args: Vec<String>, config: TelarConfig) -> (PathBuf, ResolvedPackage) {
    let (_android, rest) = split_android_flag(cargo_args);
    let backend_value = backend_as_str(config.backend.unwrap_or_default());

    let mut build_args = vec!["build".to_string()];
    build_args.extend(rest.clone());
    if !build_args.contains(&"--release".to_string()) {
        build_args.push("--release".to_string());
    }
    eprintln!("[cargo-telar] Building release binary...");
    let status = Command::new("cargo")
        .args(&build_args)
        .env("TELAR_RENDERER_BACKEND", backend_value)
        .status()
        .expect("[cargo-telar] failed to invoke cargo");
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

    let resolved = resolve_package(&rest);
    let bin_path = package_bin_path(&resolved.workspace_root, &resolved.name(), "release");
    if !bin_path.exists() {
        eprintln!(
            "[cargo-telar] Build succeeded but no binary was found at {}. Does this package produce a `[[bin]]`?",
            bin_path.display()
        );
        std::process::exit(1);
    }
    (bin_path, resolved)
}

pub(crate) fn build_desktop_dir(cargo_args: Vec<String>, config: TelarConfig) -> ! {
    let (bin_path, resolved) = run_release_build(cargo_args, config);
    let package_name = resolved.name();

    let dist_dir = dist_dir(&resolved.workspace_root).join(&package_name);
    if let Err(e) = std::fs::create_dir_all(&dist_dir) {
        eprintln!("[cargo-telar] Failed to create {}: {e}", dist_dir.display());
        std::process::exit(1);
    }
    if let Err(e) = std::fs::copy(&bin_path, dist_dir.join(&package_name)) {
        eprintln!("[cargo-telar] Failed to copy binary into dist: {e}");
        std::process::exit(1);
    }
    // Assets are embedded in the binary today (rsx has no exe-relative asset lookup), so the bundle is just the self-contained executable. See the "disk assets" TODO for the disk-asset story that would add an assets/ copy here.

    eprintln!(
        "[cargo-telar] Packaged desktop build at {}",
        dist_dir.display()
    );
    std::process::exit(0);
}

fn desktop_entry_file(name: &str, icon: bool) -> String {
    let mut entry = format!(
        "[Desktop Entry]\nType=Application\nName={name}\nExec={name}\nCategories=Utility;\n"
    );
    if icon {
        entry.push_str(&format!("Icon={name}\n"));
    }
    entry
}

#[cfg(unix)]
fn set_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Err(e) = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)) {
        eprintln!(
            "[cargo-telar] Warning: failed to set 0755 on {}: {e}",
            path.display()
        );
    }
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) {}

fn write_or_exit(path: &Path, contents: impl AsRef<[u8]>) {
    if let Err(e) = std::fs::write(path, contents) {
        eprintln!("[cargo-telar] Failed to write {}: {e}", path.display());
        std::process::exit(1);
    }
}

fn create_dir_or_exit(path: &Path) {
    if let Err(e) = std::fs::create_dir_all(path) {
        eprintln!("[cargo-telar] Failed to create {}: {e}", path.display());
        std::process::exit(1);
    }
}

fn stage_binary(bin_path: &Path, dest: &Path) {
    if let Err(e) = std::fs::copy(bin_path, dest) {
        eprintln!("[cargo-telar] Failed to copy binary into staging: {e}");
        std::process::exit(1);
    }
    set_executable(dest);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_entry_includes_icon_only_when_requested() {
        let plain = desktop_entry_file("myapp", false);
        assert!(plain.contains("[Desktop Entry]"));
        assert!(plain.contains("Type=Application"));
        assert!(plain.contains("Name=myapp"));
        assert!(plain.contains("Exec=myapp"));
        assert!(plain.contains("Categories=Utility;"));
        assert!(!plain.contains("Icon="));

        let with_icon = desktop_entry_file("myapp", true);
        assert!(with_icon.contains("Icon=myapp"));
    }
}

/// The message for a tool the build needs and cannot find.
pub(crate) fn tool_missing(tool: &str, install: &str) -> String {
    format!("`{tool}` is not installed, and the web build needs it.\n  Install it with: {install}")
}
