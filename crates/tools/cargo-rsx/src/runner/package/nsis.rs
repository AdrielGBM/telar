use std::path::Path;
use std::process::Command;

use super::super::config::{RsxConfig, read_package_manifest, split_android_flag};
use super::{create_dir_or_exit, run_release_build, write_or_exit};

fn nsis_script(name: &str, bin_path: &Path, installer_path: &Path) -> String {
    format!(
        r#"Name "{name}"
OutFile "{installer}"
InstallDir "$PROGRAMFILES64\{name}"
RequestExecutionLevel admin

Page directory
Page instfiles

Section "Install"
    SetOutPath $INSTDIR
    File "/oname={name}.exe" "{bin}"
    CreateShortcut "$SMPROGRAMS\{name}.lnk" "$INSTDIR\{name}.exe"
    WriteUninstaller "$INSTDIR\uninstall.exe"
SectionEnd

Section "Uninstall"
    Delete "$INSTDIR\{name}.exe"
    Delete "$INSTDIR\uninstall.exe"
    Delete "$SMPROGRAMS\{name}.lnk"
    RMDir "$INSTDIR"
SectionEnd
"#,
        installer = installer_path.display(),
        bin = bin_path.display(),
    )
}

pub(crate) fn build_nsis(cargo_args: Vec<String>, config: RsxConfig) -> ! {
    // The installer wraps the host-built .exe, so it must be produced on a Windows host (no cross-compilation).
    if !cfg!(target_os = "windows") {
        eprintln!(
            "[cargo-rsx] `--format nsis` must run on Windows (it packages the host-built .exe with makensis)."
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
    let staging = dist_dir.join("nsis-staging");
    let _ = std::fs::remove_dir_all(&staging);
    create_dir_or_exit(&staging);
    let installer_path = dist_dir.join(format!("{package_name}_{version}_setup.exe"));
    let script_path = staging.join(format!("{package_name}.nsi"));
    write_or_exit(
        &script_path,
        nsis_script(&package_name, &bin_path, &installer_path),
    );

    let result = Command::new("makensis").arg(&script_path).status();
    match result {
        Ok(status) if status.success() => {
            eprintln!(
                "[cargo-rsx] Packaged installer at {}",
                installer_path.display()
            );
            std::process::exit(0);
        }
        Ok(status) => std::process::exit(status.code().unwrap_or(1)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!(
                "[cargo-rsx] `makensis` is required for --format nsis but was not found on PATH. Install NSIS (winget install NSIS.NSIS)."
            );
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("[cargo-rsx] Failed to invoke makensis: {e}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nsis_script_installs_and_uninstalls_the_renamed_exe() {
        let script = nsis_script(
            "demo",
            Path::new("C:\\repo\\target\\release\\demo.exe"),
            Path::new("C:\\repo\\target\\rsx-dist\\demo_1.0_setup.exe"),
        );
        assert!(script.contains("OutFile \"C:\\repo\\target\\rsx-dist\\demo_1.0_setup.exe\""));
        assert!(
            script.contains("File \"/oname=demo.exe\" \"C:\\repo\\target\\release\\demo.exe\"")
        );
        assert!(script.contains("InstallDir \"$PROGRAMFILES64\\demo\""));
        assert!(script.contains("Delete \"$INSTDIR\\demo.exe\""));
    }
}
