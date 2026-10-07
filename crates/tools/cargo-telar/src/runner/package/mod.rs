//! The bundlers, and the release build they all share.

use std::path::{Path, PathBuf};
use std::process::Command;

use super::config::{
    ResolvedPackage, TelarSection, backend_as_str, resolve_package, split_android_flag,
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
pub(crate) use web::{WASM_TARGET, build_web, build_web_bundle, compressible, media_type};

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
    telar_project::find_target_dir(workspace_root)
        .join(profile)
        .join(file)
}

pub(crate) fn package_bin_path(
    workspace_root: &Path,
    package_name: &str,
    profile: &str,
) -> PathBuf {
    // EXE_SUFFIX so `dir` packaging and the hot-reload spawn find `<name>.exe` on Windows.
    telar_project::find_target_dir(workspace_root)
        .join(profile)
        .join(format!("{package_name}{}", std::env::consts::EXE_SUFFIX))
}

// Under the resolved target dir, next to the binary they package, so they inherit its gitignore and are never confused with the generated `.rsx/` source.
pub(crate) fn dist_dir(workspace_root: &Path) -> PathBuf {
    telar_project::find_target_dir(workspace_root).join("telar-dist")
}

// The release/debug profile cargo emits into, mirroring build_cargo_args's `--release` handling.
pub(crate) fn profile_of(args: &[String]) -> &'static str {
    if args.contains(&"--release".to_string()) {
        "release"
    } else {
        "debug"
    }
}

// Reports success, forwards the tool's exit code, or exits 1 with an install hint when the binary is missing. Diverges, since every bundler ends here.
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

// Returns the built binary path alongside the resolved package: workspace root plus the manifest fields the bundlers read.
fn run_release_build(cargo_args: Vec<String>, config: TelarSection) -> (PathBuf, ResolvedPackage) {
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

pub(crate) fn build_desktop_dir(cargo_args: Vec<String>, config: TelarSection) -> ! {
    let (bin_path, resolved) = run_release_build(cargo_args, config);
    let package_name = resolved.name();

    let dist_dir = dist_dir(&resolved.workspace_root).join(&package_name);
    stage_desktop_dir(&dist_dir, &bin_path, &resolved.package_dir, &package_name);

    eprintln!(
        "[cargo-telar] Packaged desktop build at {}",
        dist_dir.display()
    );
    std::process::exit(0);
}

/// The executable, with the icon licence notice beside it. Assets are embedded in the binary, so nothing else ships.
fn stage_desktop_dir(dist_dir: &Path, bin_path: &Path, package_dir: &Path, package_name: &str) {
    create_dir_or_exit(dist_dir);
    stage_binary(bin_path, &dist_dir.join(package_name));
    stage_icon_notice(package_dir, dist_dir);
}

/// The licence notice of the icons baked into the package at `package_dir` and into the crates it is built with, when they baked any.
pub(crate) fn icon_notice(package_dir: &Path) -> Option<PathBuf> {
    let notice = package_dir
        .join(".telar")
        .join(telar_baker::ICONS_NOTICE_FILENAME);
    notice.is_file().then_some(notice)
}

/// Copies the [`icon_notice`] into `dir`, where the format keeps third-party notices, answering where it landed. Nothing ships when the bake wrote none.
fn stage_icon_notice(package_dir: &Path, dir: &Path) -> Option<PathBuf> {
    copy_icon_notice(package_dir, &dir.join(telar_baker::ICONS_NOTICE_FILENAME))
}

/// Copies the [`icon_notice`] to `to`, answering `to`, or `None` when the bake wrote none.
pub(crate) fn copy_icon_notice(package_dir: &Path, to: &Path) -> Option<PathBuf> {
    let notice = icon_notice(package_dir)?;
    if let Some(dir) = to.parent() {
        create_dir_or_exit(dir);
    }
    if let Err(e) = std::fs::copy(&notice, to) {
        eprintln!(
            "[cargo-telar] Failed to copy {} to {}: {e}",
            notice.display(),
            to.display()
        );
        std::process::exit(1);
    }
    Some(to.to_path_buf())
}

/// Where the icon licence notice of an APK lands: beside it, since cargo-apk packs only the `assets` directory the package's own manifest names and nothing can be added to a signed APK without signing it again.
pub(crate) fn apk_notice_path(dist_dir: &Path, package_name: &str) -> PathBuf {
    dist_dir.join(format!(
        "{package_name}-{}",
        telar_baker::ICONS_NOTICE_FILENAME
    ))
}

/// Where a Linux package keeps a package's documentation, licences among it: `/usr/share/doc/<name>/` under the root it installs from.
fn doc_dir(root: &Path, package_name: &str) -> PathBuf {
    root.join("usr")
        .join("share")
        .join("doc")
        .join(package_name)
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
#[path = "package_test.rs"]
mod tests;

/// The message for a tool the build needs and cannot find.
pub(crate) fn tool_missing(tool: &str, install: &str) -> String {
    format!("`{tool}` is not installed, and the web build needs it.\n  Install it with: {install}")
}
