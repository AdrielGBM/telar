use super::*;

#[test]
fn nsis_script_installs_and_uninstalls_the_renamed_exe() {
    let script = nsis_script(
        "demo",
        Path::new("C:\\repo\\target\\release\\demo.exe"),
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
}
