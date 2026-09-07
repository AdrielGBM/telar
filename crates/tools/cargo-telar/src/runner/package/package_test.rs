use super::*;

#[test]
fn desktop_entry_includes_icon_only_when_requested() {
    let plain = desktop_entry_file("myapp", false);
    assert!(plain.contains("[Desktop Entry]"), "{plain}");
    assert!(plain.contains("Type=Application"), "{plain}");
    assert!(plain.contains("Name=myapp"), "{plain}");
    assert!(plain.contains("Exec=myapp"), "{plain}");
    assert!(plain.contains("Categories=Utility;"), "{plain}");
    assert!(
        !plain.contains("Icon="),
        "an entry with no icon requested must not name one:\n{plain}"
    );

    let with_icon = desktop_entry_file("myapp", true);
    assert!(with_icon.contains("Icon=myapp"), "{with_icon}");
}
