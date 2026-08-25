use platform_core::WindowConfig;

#[derive(Clone)]
pub struct AppConfig {
    pub window: WindowConfig,
    pub font_paths: Vec<std::path::PathBuf>,
    pub font_data: Vec<Vec<u8>>,
    /// The family this application's unstyled text shapes in — a shell's theme font. `None` keeps the
    /// platform's own. Loading a face with `font_paths`/`font_data` does not choose it; this does.
    ///
    /// A property of *this* configuration, so a second surface built later renders in its own family rather
    /// than in whichever one was configured last. A single text overrides it with
    /// [`TextStyle::with_font_family`](crate::TextStyle::with_font_family).
    pub font_family: Option<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            window: WindowConfig::default(),
            font_paths: Vec::new(),
            font_data: Vec::new(),
            font_family: None,
        }
    }
}

impl From<WindowConfig> for AppConfig {
    fn from(window: WindowConfig) -> Self {
        Self {
            window,
            ..Self::default()
        }
    }
}
