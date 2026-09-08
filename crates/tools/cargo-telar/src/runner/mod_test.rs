use super::*;

/// Read from the `cargo-telar` package itself, which declares no `[features]` at all — the shape of a project that has named no frontend of its own, and the one case where a target has to be reached through the dependency.
#[test]
fn a_package_that_declares_no_frontend_reaches_one_through_telar() {
    assert_eq!(
        frontend_args(Target::Tui, None, &[]),
        vec!["--features", "telar/tui"]
    );
    assert_eq!(
        frontend_args(Target::Desktop, None, &[]),
        vec!["--features", "telar/desktop"]
    );
}
