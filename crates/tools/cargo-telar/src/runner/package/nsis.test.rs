use super::*;

#[test]
fn nsis_script_installs_and_uninstalls_the_renamed_exe() {
    let script = nsis_script(
        "demo",
        Path::new("C:\\repo\\target\\release\\demo.exe"),
        None,
        Path::new("C:\\repo\\target\\telar-dist\\demo_1.0_setup.exe"),
    );
    assert!(
        script.contains("OutFile \"C:\\repo\\target\\telar-dist\\demo_1.0_setup.exe\""),
        "{script}"
    );
    assert!(
        script.contains("File \"/oname=demo.exe\" \"C:\\repo\\target\\release\\demo.exe\""),
        "the installer must place the renamed exe:\n{script}"
    );
    assert!(
        script.contains("InstallDir \"$PROGRAMFILES64\\demo\""),
        "{script}"
    );
    assert!(
        script.contains("Delete \"$INSTDIR\\demo.exe\""),
        "the uninstaller must remove the same name it installed:\n{script}"
    );
    assert!(
        !script.contains("ICONS-LICENSES.txt"),
        "a package that baked no icon installs no notice:\n{script}"
    );
}

#[test]
fn the_icon_notice_is_installed_beside_the_exe_and_uninstalled() {
    let staging = super::super::tests::Staging::new("nsis", true);
    let script_path = stage_nsis(
        &staging.out,
        &staging.bin_path,
        &staging.package_dir,
        "demo",
        Path::new("C:\\out\\demo_setup.exe"),
    );
    let script = std::fs::read_to_string(&script_path).unwrap();
    let staged = staging.out.join("ICONS-LICENSES.txt");
    assert_eq!(
        staging.notice_at("ICONS-LICENSES.txt").as_deref(),
        Some(super::super::tests::NOTICE)
    );
    assert!(
        script.contains(&format!(
            "    File \"/oname=ICONS-LICENSES.txt\" \"{}\"\n",
            staged.display()
        )),
        "{script}"
    );
    assert!(
        script.contains("    Delete \"$INSTDIR\\ICONS-LICENSES.txt\"\n"),
        "{script}"
    );
    let installed = script.find("ICONS-LICENSES.txt").unwrap();
    assert!(
        installed > script.find("SetOutPath $INSTDIR").unwrap(),
        "the notice is installed into the install directory:\n{script}"
    );
}
