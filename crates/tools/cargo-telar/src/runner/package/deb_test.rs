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
