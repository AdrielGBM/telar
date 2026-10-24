//! Building, installing and launching on an Android device.

use std::path::{Path, PathBuf};
use std::process::Command;

use super::config::{
    TelarSection, backend_as_str, default_app_id, read_package_manifest, resolve_package,
    split_android_flag,
};
use super::package::{apk_notice_path, copy_icon_notice, dist_dir, profile_of};

pub(crate) fn resolve_ndk_root() -> Option<String> {
    if let Ok(v) = std::env::var("ANDROID_NDK_ROOT")
        && !v.is_empty()
    {
        return Some(v);
    }
    let android_home = std::env::var("ANDROID_HOME").ok()?;
    let ndk_dir = Path::new(&android_home).join("ndk");
    let mut versions: Vec<_> = std::fs::read_dir(&ndk_dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .collect();
    versions.sort_by_key(|e| e.file_name());
    Some(versions.last()?.path().to_string_lossy().into_owned())
}

fn android_package_id(args: &[String]) -> String {
    let pkg = read_package_manifest(args);
    if let Some(id) = pkg
        .as_ref()
        .and_then(|p| p.metadata.as_ref())
        .and_then(|m| m.android.as_ref())
        .and_then(|a| a.package.as_deref())
    {
        return id.to_owned();
    }
    let crate_name = pkg.map(|p| p.name).unwrap_or_else(|| "app".to_string());
    default_app_id(&crate_name)
}

fn apk_path(args: &[String]) -> PathBuf {
    let resolved = resolve_package(args);
    let profile = profile_of(args);
    telar_project::find_target_dir(&resolved.workspace_root)
        .join(profile)
        .join("apk")
        .join(format!("{}.apk", resolved.name()))
}

fn is_keystore_path_key(key: &str) -> bool {
    key.starts_with("CARGO_APK_") && key.ends_with("_KEYSTORE")
}

fn dotenv_entries(root: &Path) -> Result<Vec<(String, String)>, String> {
    let path = root.join(".env");
    let entries = match dotenvy::from_path_iter(&path) {
        Ok(entries) => entries,
        Err(e) if e.not_found() => return Ok(Vec::new()),
        Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
    };
    entries
        .map(|entry| {
            let (key, value) = entry.map_err(|e| format!("invalid {}: {e}", path.display()))?;
            let value = if is_keystore_path_key(&key) && !value.is_empty() {
                root.join(&value).to_string_lossy().into_owned()
            } else {
                value
            };
            Ok((key, value))
        })
        .collect()
}

fn load_dotenv(cmd: &mut Command) -> Result<(), String> {
    let cwd = std::env::current_dir().unwrap_or_default();
    let root = telar_project::find_workspace_root(&cwd).unwrap_or(cwd);
    for (key, value) in dotenv_entries(&root)? {
        // Explicit env wins over .env: only set keys absent from the calling environment.
        if std::env::var_os(&key).is_none() {
            cmd.env(key, value);
        }
    }
    Ok(())
}

pub(crate) fn make_android_cmd(cargo_args: Vec<String>, config: TelarSection) -> Command {
    let ndk_root = resolve_ndk_root();
    let backend_value = backend_as_str(config.backend.unwrap_or_default());
    let mut cmd = Command::new("cargo");
    cmd.args(cargo_args)
        .env("TELAR_RENDERER_BACKEND", backend_value);
    if let Some(ndk) = ndk_root {
        cmd.env("ANDROID_NDK_ROOT", ndk);
    }
    if let Err(e) = load_dotenv(&mut cmd) {
        eprintln!("[cargo-telar] {e}");
        std::process::exit(1);
    }
    cmd
}

pub(crate) fn android_install_and_launch(args: &[String]) {
    let apk = apk_path(args);
    let status = Command::new("adb")
        .args(["install", "-r", apk.to_str().unwrap_or_default()])
        .status()
        .expect("[cargo-telar] failed to invoke adb");
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

    let package_id = android_package_id(args);
    let component = format!("{package_id}/android.app.NativeActivity");
    let status = Command::new("adb")
        .args(["shell", "am", "start", "-n", &component])
        .status()
        .expect("[cargo-telar] failed to invoke adb");
    std::process::exit(status.code().unwrap_or(1));
}

pub(crate) fn android_sdk_root() -> Option<PathBuf> {
    for var in ["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
        if let Ok(value) = std::env::var(var)
            && !value.is_empty()
        {
            return Some(PathBuf::from(value));
        }
    }
    None
}

pub(crate) fn installed_android_platforms(sdk_root: &Path) -> Vec<u32> {
    let mut versions: Vec<u32> = std::fs::read_dir(sdk_root.join("platforms"))
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let name = entry.file_name();
            name.to_str()?.strip_prefix("android-")?.parse::<u32>().ok()
        })
        .collect();
    versions.sort_unstable();
    versions
}

pub(crate) fn build_android_package(cargo_args: Vec<String>, config: TelarSection) -> ! {
    let (_android, rest) = split_android_flag(cargo_args);
    let mut build_args = vec!["apk".to_string(), "build".to_string(), "--lib".to_string()];
    build_args.extend(rest.clone());
    if !build_args.contains(&"--release".to_string()) {
        build_args.push("--release".to_string());
    }
    let status = make_android_cmd(build_args, config)
        .status()
        .expect("[cargo-telar] failed to invoke cargo");
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

    let apk = apk_path(&rest);
    let resolved = resolve_package(&rest);
    let dist_dir = dist_dir(&resolved.workspace_root);
    let _ = std::fs::create_dir_all(&dist_dir);
    if !apk.exists() {
        eprintln!(
            "[cargo-telar] APK build finished but {} was not found.",
            apk.display()
        );
        std::process::exit(1);
    }
    let dest = dist_dir.join(apk.file_name().unwrap_or_default());
    if let Err(e) = std::fs::copy(&apk, &dest) {
        eprintln!("[cargo-telar] Built APK but failed to copy into dist: {e}");
        std::process::exit(1);
    }
    eprintln!("[cargo-telar] Packaged APK at {}", dest.display());
    if let Some(notice) = copy_icon_notice(
        &resolved.package_dir,
        &apk_notice_path(&dist_dir, &resolved.name()),
    ) {
        eprintln!(
            "[cargo-telar] The licence notice of its icons is beside it, at {}: an APK carries only the `assets` directory its manifest names, so ship the notice with it or show it in the app.",
            notice.display()
        );
    }
    std::process::exit(0);
}

#[cfg(test)]
#[path = "android_test.rs"]
mod tests;
