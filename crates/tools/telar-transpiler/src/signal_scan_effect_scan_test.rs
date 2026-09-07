use super::*;

/// The spelling the scanner used to miss, and the one every application outside a `use telar::*` writes.
#[test]
fn a_path_qualified_effect_is_recognised() {
    assert_eq!(scan_effects("let e = telar::effect(|| {});"), vec!["e"]);
    assert_eq!(scan_effects("let e = crate::effect(|| {});"), vec!["e"]);
    assert_eq!(scan_effects("let e = effect(|| {});"), vec!["e"]);
}

#[test]
fn a_binding_that_is_not_an_effect_is_left_alone() {
    assert!(
        scan_effects("let s = signal(0);").is_empty(),
        "a signal is not an effect"
    );
    assert!(
        scan_effects("let m = memo(|| 1);").is_empty(),
        "nor is a memo"
    );
    assert!(
        scan_effects("let e = side_effect(|| {});").is_empty(),
        "nor is a name that merely reads like one"
    );
}
