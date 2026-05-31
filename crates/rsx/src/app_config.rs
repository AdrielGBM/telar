use platform_core::WindowConfig;

pub struct AppConfig {
    pub window: WindowConfig,
    pub font_paths: Vec<std::path::PathBuf>,
    pub font_data: Vec<Vec<u8>>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            window: WindowConfig::default(),
            font_paths: Vec::new(),
            font_data: Vec::new(),
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
