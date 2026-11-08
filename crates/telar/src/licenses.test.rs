use super::{icon_licenses, install_icon_licenses};

#[test]
fn the_installed_notice_is_read_back_and_a_later_install_replaces_it() {
    assert_eq!(icon_licenses(), None);

    install_icon_licenses("Icons in demo\n");
    assert_eq!(icon_licenses(), Some("Icons in demo\n"));

    install_icon_licenses("Icons in other\n");
    assert_eq!(icon_licenses(), Some("Icons in other\n"));
}
