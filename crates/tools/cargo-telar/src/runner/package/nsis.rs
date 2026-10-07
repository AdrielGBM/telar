//! Packaging a Windows NSIS installer.

use std::path::{Path, PathBuf};
use std::process::Command;

use super::super::config::TelarSection;
use super::{
    create_dir_or_exit, dist_dir, run_bundler_tool, run_release_build, stage_icon_notice,
    write_or_exit,
};

/// The installer script: the renamed exe, and the icon licence notice beside it in the install directory when `notice` names one, each removed again by the uninstaller.
fn nsis_script(
    name: &str,
    bin_path: &Path,
    notice: Option<&Path>,
    installer_path: &Path,
) -> String {
    let notice_file = telar_baker::ICONS_NOTICE_FILENAME;
    let (install_notice, uninstall_notice) = match notice {
        Some(notice) => (
            format!(
                "    File \"/oname={notice_file}\" \"{}\"\n",
                notice.display()
            ),
            format!("    Delete \"$INSTDIR\\{notice_file}\"\n"),
        ),
        None => (String::new(), String::new()),
    };
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
{install_notice}    CreateShortcut "$SMPROGRAMS\{name}.lnk" "$INSTDIR\{name}.exe"
    WriteUninstaller "$INSTDIR\uninstall.exe"
SectionEnd

Section "Uninstall"
    Delete "$INSTDIR\{name}.exe"
{uninstall_notice}    Delete "$INSTDIR\uninstall.exe"
    Delete "$SMPROGRAMS\{name}.lnk"
    RMDir "$INSTDIR"
SectionEnd
"#,
        installer = installer_path.display(),
        bin = bin_path.display(),
    )
}

/// The staging directory `makensis` reads: a copy of the icon licence notice taken at build time, and the script that installs it beside the exe. Answers the script's path.
fn stage_nsis(
    staging: &Path,
    bin_path: &Path,
    package_dir: &Path,
    package_name: &str,
    installer_path: &Path,
) -> PathBuf {
    let _ = std::fs::remove_dir_all(staging);
    create_dir_or_exit(staging);
    let notice = stage_icon_notice(package_dir, staging);
    let script_path = staging.join(format!("{package_name}.nsi"));
    write_or_exit(
        &script_path,
        nsis_script(package_name, bin_path, notice.as_deref(), installer_path),
    );
    script_path
}

pub(crate) fn build_nsis(cargo_args: Vec<String>, config: TelarSection) -> ! {
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
    let installer_path = dist_dir.join(format!("{package_name}_{version}_setup.exe"));
    let script_path = stage_nsis(
        &dist_dir.join("nsis-staging"),
        &bin_path,
        &resolved.package_dir,
        &package_name,
        &installer_path,
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
#[path = "nsis_test.rs"]
mod tests;
