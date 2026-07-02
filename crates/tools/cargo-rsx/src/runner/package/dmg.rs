use std::process::Command;

use super::super::config::{RsxConfig, read_package_manifest, split_android_flag};
use super::{create_dir_or_exit, run_release_build, stage_binary, write_or_exit};

fn info_plist(name: &str, version: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>{name}</string>
    <key>CFBundleDisplayName</key><string>{name}</string>
    <key>CFBundleIdentifier</key><string>com.example.{name}</string>
    <key>CFBundleExecutable</key><string>{name}</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleVersion</key><string>{version}</string>
    <key>CFBundleShortVersionString</key><string>{version}</string>
    <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
"#
    )
}

pub(crate) fn build_dmg(cargo_args: Vec<String>, config: RsxConfig) -> ! {
    // hdiutil only exists on macOS and rsx does not cross-compile, so the bundle must be produced on a mac host.
    if !cfg!(target_os = "macos") {
        eprintln!(
            "[cargo-rsx] `--format dmg` must run on macOS (it packages the host-built binary with hdiutil)."
        );
        std::process::exit(2);
    }
    let (_android, rest) = split_android_flag(cargo_args);
    let manifest = read_package_manifest(&rest);
    let version = manifest
        .as_ref()
        .and_then(|p| p.version.clone())
        .unwrap_or_else(|| "0.1.0".to_string());
    let (bin_path, workspace_root, package_name) = run_release_build(rest, config);

    let dist_dir = workspace_root.join("target").join("rsx-dist");
    let staging = dist_dir.join("dmg-staging");
    let _ = std::fs::remove_dir_all(&staging);
    let contents = staging.join(format!("{package_name}.app")).join("Contents");
    let macos_dir = contents.join("MacOS");
    create_dir_or_exit(&macos_dir);
    write_or_exit(
        &contents.join("Info.plist"),
        info_plist(&package_name, &version),
    );
    stage_binary(&bin_path, &macos_dir.join(&package_name));

    let dmg_path = dist_dir.join(format!("{package_name}_{version}.dmg"));
    let result = Command::new("hdiutil")
        .arg("create")
        .arg("-volname")
        .arg(&package_name)
        .arg("-srcfolder")
        .arg(&staging)
        .arg("-ov")
        .arg("-format")
        .arg("UDZO")
        .arg(&dmg_path)
        .status();
    match result {
        Ok(status) if status.success() => {
            eprintln!("[cargo-rsx] Packaged dmg at {}", dmg_path.display());
            std::process::exit(0);
        }
        Ok(status) => std::process::exit(status.code().unwrap_or(1)),
        Err(e) => {
            eprintln!("[cargo-rsx] Failed to invoke hdiutil: {e}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info_plist_carries_bundle_identity_and_version() {
        let plist = info_plist("demo", "1.2.3");
        assert!(plist.contains("<key>CFBundleExecutable</key><string>demo</string>"));
        assert!(plist.contains("<key>CFBundleIdentifier</key><string>com.example.demo</string>"));
        assert!(plist.contains("<key>CFBundleShortVersionString</key><string>1.2.3</string>"));
    }
}
