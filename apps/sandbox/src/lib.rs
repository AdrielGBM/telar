pub mod app;
pub mod images;
pub mod sections;
pub mod theme;

rsx::app!(
    {
        rsx::set_theme(theme::SandboxTheme::modern());
    },
    rsx::AppConfig::default(),
    app::SandboxRoot
);
