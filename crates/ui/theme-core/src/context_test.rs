use super::*;

/// The whole reason this is not an `Option`: every caller that had to handle a missing theme wrote its own flat constant, and none of them followed the mode — so the careful mode-following default was unreachable on exactly the path that runs when nobody has configured anything.
#[test]
fn an_unregistered_theme_still_follows_the_mode() {
    crate::register_mode("light", || {});
    crate::register_mode("dark", || {});
    crate::follow_system("light", "dark");

    crate::set_system_dark(false);
    let light_ink = use_theme_tokens().ink();
    crate::set_system_dark(true);
    let dark_ink = use_theme_tokens().ink();

    assert!(
        light_ink.r < 0.5,
        "dark ink on a light page, got {light_ink:?}"
    );
    assert!(
        dark_ink.r > 0.5,
        "light ink on a dark page, got {dark_ink:?}"
    );
    crate::set_system_dark(false);
}

#[derive(Clone)]
struct Blue;
impl ThemeTokens for Blue {
    fn primary(&self) -> Color {
        Color::rgba(0.0, 0.0, 1.0, 1.0)
    }
}

/// A theme answering one question keeps the table's answers for the rest, which is what makes `impl ThemeTokens for MyTheme {}` a valid theme.
#[test]
fn a_registered_theme_overrides_only_what_it_answers() {
    set_theme(Blue);
    assert_eq!(
        use_theme_tokens().primary(),
        Color::rgba(0.0, 0.0, 1.0, 1.0)
    );
    assert_eq!(use_theme_tokens().radius(), DefaultTokens.radius());
    THEME_TOKENS.with(|s| s.set(None));
    THEME.with(|s| s.set(None));
}
