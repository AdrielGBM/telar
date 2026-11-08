use super::*;

#[test]
fn clean_cargo_and_clean_report_exit_zero() {
    assert_eq!(exit_code(Some(0), false), 0);
}

#[test]
fn report_errors_fail_even_when_cargo_succeeds() {
    assert_eq!(exit_code(Some(0), true), 1);
}

#[test]
fn cargo_failure_code_is_passed_through() {
    assert_eq!(exit_code(Some(101), false), 101);
    assert_eq!(exit_code(Some(101), true), 101);
}

#[test]
fn cargo_killed_by_signal_fails() {
    assert_eq!(exit_code(None, false), 1);
    assert_eq!(exit_code(None, true), 1);
}

#[test]
fn a_package_is_checked_against_its_catalog_and_its_style_classes() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("locales")).unwrap();
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    std::fs::write(dir.path().join("locales/en.toml"), "title = \"Home\"\n").unwrap();
    std::fs::write(
        dir.path().join("src/home.rsx"),
        "[view]\n    column @missing\n        text t\"nope\"\n",
    )
    .unwrap();

    let mut report = diagnostics::Report::default();
    report.add_member_semantic(dir.path());

    let rendered = report.render(false);
    assert!(rendered.contains("`@missing`"), "{rendered}");
    assert!(rendered.contains("`nope`"), "{rendered}");
}
