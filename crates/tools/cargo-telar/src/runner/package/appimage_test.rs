use super::*;

#[test]
fn apprun_execs_the_bundled_binary() {
    let script = apprun_script("myapp");
    assert!(
        script.starts_with("#!/bin/sh\n"),
        "AppRun has to be a shell script:\n{script}"
    );
    assert!(
        script.contains("exec \"$HERE/usr/bin/myapp\" \"$@\""),
        "{script}"
    );
}

#[test]
fn the_icon_notice_lands_in_the_appdirs_doc_directory() {
    let staging = super::super::tests::Staging::new("appimage", true);
    stage_appdir(
        &staging.out,
        &staging.bin_path,
        &staging.package_dir,
        "demo",
    );
    assert!(staging.out.join("usr/bin/demo").is_file());
    assert_eq!(
        staging
            .notice_at("usr/share/doc/demo/ICONS-LICENSES.txt")
            .as_deref(),
        Some(super::super::tests::NOTICE)
    );
}
