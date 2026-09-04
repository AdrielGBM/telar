//! Packaging a Windows NSIS installer.

use std::path::Path;
use std::process::Command;

use super::super::config::TelarConfig;
use super::{create_dir_or_exit, dist_dir, run_bundler_tool, run_release_build, write_or_exit};

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

pub(crate) fn build_nsis(cargo_args: Vec<String>, config: TelarConfig) -> ! {
    // The installer wraps the host-built .exe, so it must be produced on a Windows host (no cross-compilation).
    if !cfg!(target_os = "windows") {
        eprintln!(
            "[cargo-telar] `--format nsis` must run on Windows (it packages the host-built .exe with makensis)."
        );
        std::process::exit(2);
    }
    let (bin_path, resolved) = run_release_build(cargo_args, config);
    let package_name = resolved.name();
    let version = resolved.version();

    let dist_dir = dist_dir(&resolved.workspace_root);
    let staging = dist_dir.join("nsis-staging");
    let _ = std::fs::remove_dir_all(&staging);
    create_dir_or_exit(&staging);
    let installer_path = dist_dir.join(format!("{package_name}_{version}_setup.exe"));
    let script_path = staging.join(format!("{package_name}.nsi"));
    write_or_exit(
        &script_path,
        nsis_script(&package_name, &bin_path, &installer_path),
    );

    let mut cmd = Command::new("makensis");
    cmd.arg(&script_path);
    run_bundler_tool(
        &mut cmd,
        "installer",
        &installer_path,
        Some(
            "[cargo-telar] `makensis` is required for --format nsis but was not found on PATH. Install NSIS (winget install NSIS.NSIS).",
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nsis_script_installs_and_uninstalls_the_renamed_exe() {
        let script = nsis_script(
            "demo",
            Path::new("C:\\repo\\target\\release\\demo.exe"),
            Path::new("C:\\repo\\target\\telar-dist\\demo_1.0_setup.exe"),
        );
        assert!(script.contains("OutFile \"C:\\repo\\target\\telar-dist\\demo_1.0_setup.exe\""));
        assert!(
            script.contains("File \"/oname=demo.exe\" \"C:\\repo\\target\\release\\demo.exe\"")
        );
        assert!(script.contains("InstallDir \"$PROGRAMFILES64\\demo\""));
        assert!(script.contains("Delete \"$INSTDIR\\demo.exe\""));
    }
}
