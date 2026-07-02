use std::path::{Path, PathBuf};
use std::process::Command;

use super::config::{
    RsxConfig, backend_as_str, find_package_dir, read_package_manifest, split_android_flag,
};

mod appimage;
mod deb;
mod dmg;
mod nsis;

pub(crate) use appimage::build_appimage;
pub(crate) use deb::build_deb;
pub(crate) use dmg::build_dmg;
pub(crate) use nsis::build_nsis;

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

// Runs the shared release build for the desktop packaging formats (dir/deb/appimage) and returns the built binary path, the workspace root that hosts the dist bundle, and the package name.
fn run_release_build(cargo_args: Vec<String>, config: RsxConfig) -> (PathBuf, PathBuf, String) {
    let (_android, rest) = split_android_flag(cargo_args);
    let backend_value = backend_as_str(config.backend.unwrap_or_default());

    let mut build_args = vec!["build".to_string()];
    build_args.extend(rest.clone());
    if !build_args.contains(&"--release".to_string()) {
        build_args.push("--release".to_string());
    }
    eprintln!("[cargo-rsx] Building release binary...");
    let status = Command::new("cargo")
        .args(&build_args)
        .env("RSX_RENDERER_BACKEND", backend_value)
        .status()
        .expect("[cargo-rsx] failed to invoke cargo");
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

    let package_dir = find_package_dir(&rest);
    let workspace_root =
        rsx_workspace::find_workspace_root(&package_dir).unwrap_or(package_dir.clone());
    let package_name = read_package_manifest(&rest)
        .map(|p| p.name)
        .unwrap_or_else(|| "app".to_string());
    let bin_path = package_bin_path(&workspace_root, &package_name, "release");
    if !bin_path.exists() {
        eprintln!(
            "[cargo-rsx] Build succeeded but no binary was found at {}. Does this package produce a `[[bin]]`?",
            bin_path.display()
        );
        std::process::exit(1);
    }
    (bin_path, workspace_root, package_name)
}

pub(crate) fn build_desktop_dir(cargo_args: Vec<String>, config: RsxConfig) -> ! {
    let (bin_path, workspace_root, package_name) = run_release_build(cargo_args, config);

    // Distribution bundle lives under target/ (a build artifact, next to the binary it packages), so it inherits target's gitignore and is not confused with .rsx/ (which is generated source).
    let dist_dir = workspace_root
        .join("target")
        .join("rsx-dist")
        .join(&package_name);
    if let Err(e) = std::fs::create_dir_all(&dist_dir) {
        eprintln!("[cargo-rsx] Failed to create {}: {e}", dist_dir.display());
        std::process::exit(1);
    }
    if let Err(e) = std::fs::copy(&bin_path, dist_dir.join(&package_name)) {
        eprintln!("[cargo-rsx] Failed to copy binary into dist: {e}");
        std::process::exit(1);
    }
    // Assets are embedded in the binary today (rsx has no exe-relative asset lookup), so the bundle is just the self-contained executable. See the "disk assets" TODO for the disk-asset story that would add an assets/ copy here.

    eprintln!(
        "[cargo-rsx] Packaged desktop build at {}",
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
            "[cargo-rsx] Warning: failed to set 0755 on {}: {e}",
            path.display()
        );
    }
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) {}

fn write_or_exit(path: &Path, contents: impl AsRef<[u8]>) {
    if let Err(e) = std::fs::write(path, contents) {
        eprintln!("[cargo-rsx] Failed to write {}: {e}", path.display());
        std::process::exit(1);
    }
}

fn create_dir_or_exit(path: &Path) {
    if let Err(e) = std::fs::create_dir_all(path) {
        eprintln!("[cargo-rsx] Failed to create {}: {e}", path.display());
        std::process::exit(1);
    }
}

fn stage_binary(bin_path: &Path, dest: &Path) {
    if let Err(e) = std::fs::copy(bin_path, dest) {
        eprintln!("[cargo-rsx] Failed to copy binary into staging: {e}");
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
