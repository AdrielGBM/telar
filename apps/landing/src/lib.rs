//! Telar's landing page, written in Telar.

telar::app!(
    theme::LandingTheme,
    {
        telar::set_theme(theme::LandingTheme::light());
    },
    telar::AppConfig::default(),
    app::LandingRoot
);

#[cfg(test)]
#[path = "lib.test.rs"]
mod tests;
