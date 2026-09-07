use super::*;

#[test]
fn the_template_substitutes_the_whole_id() {
    let transport = HttpTransport::new("https://api.iconify.design/{name}.svg");
    assert_eq!(
        transport.url_for("mdi/home"),
        "https://api.iconify.design/mdi/home.svg"
    );
}

/// Zero attempts would mean never asking at all, which is not what anybody turning retries off means.
#[test]
fn retries_cannot_be_set_below_one_attempt() {
    let transport =
        HttpTransport::new("http://example.invalid/{name}").with_retry(0, Duration::ZERO);
    assert_eq!(transport.max_attempts, 1);
}

/// Both bodies count attempts through this, so the give-up point cannot drift between targets.
#[test]
fn the_last_attempt_is_the_one_that_gives_up() {
    let error = AssetError("boom".to_string());
    assert!(
        !give_up("x", 1, 3, &error),
        "there are attempts left, so this is not the end"
    );
    assert!(!give_up("x", 2, 3, &error), "nor is the one before last");
    assert!(
        give_up("x", 3, 3, &error),
        "the last attempt is the one that gives up"
    );
}
