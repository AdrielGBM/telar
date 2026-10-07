use super::*;

#[test]
fn deb_architecture_maps_known_arches_and_passes_through_others() {
    assert_eq!(deb_architecture("x86_64"), "amd64");
    assert_eq!(deb_architecture("aarch64"), "arm64");
    assert_eq!(deb_architecture("riscv64"), "riscv64");
}

#[test]
fn deb_control_file_has_required_fields_and_trailing_newline() {
    let control = deb_control_file(
        "myapp",
        "1.2.3",
        "amd64",
        "Ada <ada@example.org>",
        "A neat app",
    );
    assert!(control.contains("Package: myapp\n"), "{control}");
    assert!(control.contains("Version: 1.2.3\n"), "{control}");
    assert!(control.contains("Architecture: amd64\n"), "{control}");
    assert!(
        control.contains("Maintainer: Ada <ada@example.org>\n"),
        "{control}"
    );
    assert!(control.contains("Description: A neat app\n"), "{control}");
    assert!(
        control.ends_with('\n'),
        "dpkg requires the control file to end with a newline:\n{control}"
    );
}

#[test]
fn the_icon_notice_lands_in_the_packages_doc_directory() {
    let staging = super::super::tests::Staging::new("deb", true);
    stage_deb(
        &staging.out,
        &staging.bin_path,
        &staging.package_dir,
        "demo",
        "Package: demo\n",
    );
    assert!(staging.out.join("usr/bin/demo").is_file());
    assert_eq!(
        staging
            .notice_at("usr/share/doc/demo/ICONS-LICENSES.txt")
            .as_deref(),
        Some(super::super::tests::NOTICE)
    );

    let plain = super::super::tests::Staging::new("deb_plain", false);
    stage_deb(
        &plain.out,
        &plain.bin_path,
        &plain.package_dir,
        "demo",
        "Package: demo\n",
    );
    assert!(!plain.out.join("usr/share/doc").exists());
}
