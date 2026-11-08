//! One test in its own binary: the notice is process-wide, so what a process reads before anything installs it is only observable where nothing else does.

#[test]
fn the_notice_is_none_until_the_application_installs_one_and_is_read_back_after() {
    assert_eq!(telar_icons::licenses(), None);

    telar::__install_icon_licenses("Icons in demo\n\nmdi — Material Design Icons\n");

    assert_eq!(
        telar_icons::licenses(),
        Some("Icons in demo\n\nmdi — Material Design Icons\n")
    );
}
