mod app;
mod images;
mod sections;
mod theme;

fn main() {
    tracing_subscriber::fmt::init();
    rsx::set_theme(theme::SandboxTheme::modern());
    rsx::run_app!(rsx::WindowConfig::default(), app::SandboxRoot);
}
