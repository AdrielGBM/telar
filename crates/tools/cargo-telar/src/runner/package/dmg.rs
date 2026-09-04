//! Packaging a macOS `.dmg`.

use std::process::Command;

use super::super::config::{TelarConfig, default_app_id};
use super::{
    create_dir_or_exit, dist_dir, run_bundler_tool, run_release_build, stage_binary, write_or_exit,
};

fn info_plist(name: &str, version: &str) -> String {
    let bundle_id = default_app_id(name);
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>{name}</string>
    <key>CFBundleDisplayName</key><string>{name}</string>
    <key>CFBundleIdentifier</key><string>{bundle_id}</string>
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

pub(crate) fn build_dmg(cargo_args: Vec<String>, config: TelarConfig) -> ! {
    // hdiutil only exists on macOS and rsx does not cross-compile, so the bundle must be produced on a mac host.
    if !cfg!(target_os = "macos") {
        eprintln!(
            "[cargo-telar] `--format dmg` must run on macOS (it packages the host-built binary with hdiutil)."
        );
        std::process::exit(2);
    }
    let (bin_path, resolved) = run_release_build(cargo_args, config);
    let package_name = resolved.name();
    let version = resolved.version();

    let dist_dir = dist_dir(&resolved.workspace_root);
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
    let mut cmd = Command::new("hdiutil");
    cmd.arg("create")
        .arg("-volname")
        .arg(&package_name)
        .arg("-srcfolder")
        .arg(&staging)
        .arg("-ov")
        .arg("-format")
        .arg("UDZO")
        .arg(&dmg_path);
    // No install hint: hdiutil ships with every macOS host and this format already refuses to run off-mac.
    run_bundler_tool(&mut cmd, "dmg", &dmg_path, None)
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
