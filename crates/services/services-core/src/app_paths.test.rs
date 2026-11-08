use super::*;

/// Nothing installed is the preview/headless case, and it must answer `None` rather than guess a real path.
#[test]
fn an_app_directory_is_none_until_a_runner_installs_one() {
    if installed().is_none() {
        assert_eq!(cache(), None);
        assert_eq!(runtime(), None);
    }
}
