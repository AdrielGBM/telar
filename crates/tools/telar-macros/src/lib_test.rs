use super::stale_cli_message;

/// The failure this catches was silent: an installed `cargo-telar` older than the `telar` it drives asks for a hot-reload build the way it used to — an environment variable — the macro builds a plain application instead, and the only thing said about it is that the app never connected to a channel it was never compiled to open.
#[test]
fn an_older_cli_asking_the_old_way_is_named() {
    let message = stale_cli_message(true).expect("this test binary has no `hot-reload` feature");

    assert!(
        message.contains("cargo install cargo-telar"),
        "the message has to name what fixes it: {message}"
    );
    assert!(
        message.contains(env!("CARGO_PKG_VERSION")),
        "and which version to install: {message}"
    );
}

/// A current CLI names the feature and sets no such variable, which is every build that is not the mismatch above.
#[test]
fn a_build_that_never_asked_is_left_alone() {
    assert_eq!(stale_cli_message(false), None);
}
