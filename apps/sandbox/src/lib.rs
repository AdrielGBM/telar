pub mod app;
pub mod images;
pub mod sections;
pub mod theme;
pub mod utils;

rsx::app!(
    theme::SandboxTheme,
    {
        rsx::set_theme_with_widgets(theme::SandboxTheme::modern());
    },
    rsx::AppConfig::default(),
    app::SandboxRoot
);
