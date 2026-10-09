telar::app!(
    theme::AppTheme,
    {
        theme::register_modes();
        telar::set_theme(theme::AppTheme::light());
    },
    telar::AppConfig::default(),
    app::Root
);

#[cfg(all(test, feature = "tooling"))]
#[path = "lib_test.rs"]
mod tests;
