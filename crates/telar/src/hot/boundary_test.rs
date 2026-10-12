use super::*;

/// Host code the dylib calls into answers as an absent service would rather than unwinding into the dylib.
#[test]
fn a_host_service_that_panics_answers_with_nothing() {
    struct Broken;
    impl Clipboard for Broken {
        fn text(&self) -> Option<String> {
            panic!("the selection owner went away")
        }
        fn set_text(&self, _text: &str) {
            panic!("the selection owner went away")
        }
    }
    let clipboard = ShieldedClipboard(Arc::new(Broken));
    assert_eq!(clipboard.text(), None);
    clipboard.set_text("kept on this side");
}

/// A library built with other features lays `HostServices` out differently, and must be refused instead of driven.
#[test]
fn a_library_with_another_layout_is_refused_with_what_to_do() {
    assert_eq!(check_guest(Some(GUEST_ABI), Some(GUEST_LAYOUT)), Ok(()));

    let other = BoundaryLayout(GUEST_LAYOUT.0 ^ 1);
    let refused = check_guest(Some(GUEST_ABI), Some(other)).expect_err("another layout is refused");
    assert!(refused.contains("restart the session"), "{refused}");
    assert!(refused.contains("matching features"), "{refused}");

    let missing = check_guest(Some(GUEST_ABI), None).expect_err("no layout is refused");
    assert!(missing.contains("restart the session"), "{missing}");
}

#[test]
fn a_library_of_another_abi_is_refused() {
    let refused = check_guest(Some(GUEST_ABI - 1), Some(GUEST_LAYOUT)).expect_err("refused");
    assert!(refused.contains("restart the session"), "{refused}");
    assert!(check_guest(None, None).is_err());
}
