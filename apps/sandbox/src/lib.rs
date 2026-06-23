pub mod app;
pub mod demo_images;
pub mod theme;

rsx::app!(
    theme::SandboxTheme,
    {
        rsx::set_theme_with_widgets(theme::SandboxTheme::modern());
    },
    rsx::AppConfig::default(),
    app::SandboxRoot
);
