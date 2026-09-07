use super::*;

#[test]
fn a_leading_tilde_becomes_home_and_nothing_else_moves() {
    let absolute = Path::new("/etc/hosts");
    assert_eq!(expand_tilde(absolute), absolute);
    let relative = Path::new("pictures/a.png");
    assert_eq!(expand_tilde(relative), relative);
    // A `~` inside the path is a directory literally called `~`, not a home to expand.
    let inner = Path::new("/tmp/~/x");
    assert_eq!(expand_tilde(inner), inner);
}

#[test]
fn a_user_dirs_assignment_is_read_past_its_quotes_and_comments() {
    let text =
        "# generated\nXDG_PICTURES_DIR=\"$HOME/Imágenes\"\nXDG_VIDEOS_DIR=\"$HOME/Vídeos\"\n";
    assert_eq!(
        parse_user_dirs(text, "XDG_PICTURES_DIR").as_deref(),
        Some("$HOME/Imágenes")
    );
    assert_eq!(parse_user_dirs(text, "XDG_MUSIC_DIR"), None);
}

#[test]
fn a_commented_assignment_is_not_an_answer() {
    let text = "#XDG_PICTURES_DIR=\"$HOME/wrong\"\nXDG_PICTURES_DIR=\"$HOME/right\"\n";
    assert_eq!(
        parse_user_dirs(text, "XDG_PICTURES_DIR").as_deref(),
        Some("$HOME/right")
    );
}

#[test]
fn the_xdg_rule_prefers_the_variable_then_home_then_the_bare_fallback() {
    assert_eq!(
        resolve_base(Some("/x".into()), Some("/home/u".into()), ".cache"),
        PathBuf::from("/x")
    );
    // An empty variable is not an answer: it is how an unset one presents through the shell.
    assert_eq!(
        resolve_base(Some("".into()), Some("/home/u".into()), ".cache"),
        PathBuf::from("/home/u/.cache")
    );
    assert_eq!(resolve_base(None, None, ".cache"), PathBuf::from(".cache"));
}

/// Nothing installed is the preview/headless case, and it must answer `None` rather than guess a real path.
#[test]
fn an_app_directory_is_none_until_a_runner_installs_one() {
    if INSTALLED.get().is_none() {
        assert_eq!(cache(), None);
        assert_eq!(runtime(), None);
    }
}
