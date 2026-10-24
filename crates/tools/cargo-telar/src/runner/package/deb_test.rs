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

#[test]
fn a_multi_line_description_gets_continuation_lines() {
    let control = deb_control_file(
        "app",
        "1",
        "amd64",
        "A <a@b.c>",
        "Synopsis\nfirst\n\nsecond\n",
    );
    assert!(
        control.ends_with("Description: Synopsis\n first\n .\n second\n"),
        "{control}"
    );
}

#[test]
fn package_names_follow_debian_rules() {
    for ok in ["my-app", "a1", "app+x.y", "7zip"] {
        assert!(validate_package_name(ok).is_ok(), "{ok}");
    }
    for bad in [
        "", "a", "My-App", "-app", ".app", "my_app", "app name", "äpp",
    ] {
        assert!(validate_package_name(bad).is_err(), "{bad}");
    }
}

#[test]
fn descriptions_need_a_synopsis_and_no_control_characters() {
    assert!(validate_description("Fine\n\nbody").is_ok());
    assert!(validate_description("  \n").is_err());
    assert!(validate_description("bad\u{0}char").is_err());
}
