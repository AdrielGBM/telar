use std::process::Command;

use super::super::config::{RsxConfig, read_package_manifest, split_android_flag};
use super::{
    create_dir_or_exit, desktop_entry_file, run_release_build, stage_binary, write_or_exit,
};

// Maps Rust's target arch name to the Debian architecture label; unknown arches pass through unchanged.
fn deb_architecture(arch: &str) -> &str {
    match arch {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => other,
    }
}

fn deb_control_file(name: &str, version: &str, arch: &str, description: &str) -> String {
    format!(
        "Package: {name}\nVersion: {version}\nArchitecture: {arch}\nMaintainer: {name} maintainers <maintainer@example.invalid>\nDescription: {description}\n"
    )
}

pub(crate) fn build_deb(cargo_args: Vec<String>, config: RsxConfig) -> ! {
    let (_android, rest) = split_android_flag(cargo_args);
    let manifest = read_package_manifest(&rest);
    let version = manifest
        .as_ref()
        .and_then(|p| p.version.clone())
        .unwrap_or_else(|| "0.1.0".to_string());
    let description = manifest.and_then(|p| p.description);
    let (bin_path, workspace_root, package_name) = run_release_build(rest, config);
    let description = description.unwrap_or_else(|| format!("{package_name} (built with rsx)"));
    let arch = deb_architecture(std::env::consts::ARCH);

    let dist_dir = workspace_root.join("target").join("rsx-dist");
    let staging = dist_dir.join("deb-staging").join(&package_name);
    // Clear any previous staging so stale files never leak into the package.
    let _ = std::fs::remove_dir_all(&staging);

    let debian_dir = staging.join("DEBIAN");
    let bin_dir = staging.join("usr").join("bin");
    let apps_dir = staging.join("usr").join("share").join("applications");
    create_dir_or_exit(&debian_dir);
    create_dir_or_exit(&bin_dir);
    create_dir_or_exit(&apps_dir);

    write_or_exit(
        &debian_dir.join("control"),
        deb_control_file(&package_name, &version, arch, &description),
    );
    stage_binary(&bin_path, &bin_dir.join(&package_name));
    write_or_exit(
        &apps_dir.join(format!("{package_name}.desktop")),
        desktop_entry_file(&package_name, false),
    );

    let deb_path = dist_dir.join(format!("{package_name}_{version}_{arch}.deb"));
    let result = Command::new("dpkg-deb")
        .arg("--build")
        .arg("--root-owner-group")
        .arg(&staging)
        .arg(&deb_path)
        .status();
    match result {
        Ok(status) if status.success() => {
            eprintln!("[cargo-rsx] Packaged deb at {}", deb_path.display());
            std::process::exit(0);
        }
        Ok(status) => std::process::exit(status.code().unwrap_or(1)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!(
                "[cargo-rsx] `dpkg-deb` is required for --format deb but was not found on PATH. On NixOS: `nix shell nixpkgs#dpkg`."
            );
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("[cargo-rsx] Failed to invoke dpkg-deb: {e}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deb_architecture_maps_known_arches_and_passes_through_others() {
        assert_eq!(deb_architecture("x86_64"), "amd64");
        assert_eq!(deb_architecture("aarch64"), "arm64");
        assert_eq!(deb_architecture("riscv64"), "riscv64");
    }

    #[test]
    fn deb_control_file_has_required_fields_and_trailing_newline() {
        let control = deb_control_file("myapp", "1.2.3", "amd64", "A neat app");
        assert!(control.contains("Package: myapp\n"));
        assert!(control.contains("Version: 1.2.3\n"));
        assert!(control.contains("Architecture: amd64\n"));
        assert!(control.contains("Maintainer: myapp maintainers <maintainer@example.invalid>\n"));
        assert!(control.contains("Description: A neat app\n"));
        assert!(control.ends_with('\n'));
    }
}
