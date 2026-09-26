//! The configuration an application starts with: its window, its fonts and its renderer.

use platform_core::WindowConfig;
use renderer_core::FontAsset;

#[derive(Clone, Default)]
/// What an application starts with: its window, its fonts and its renderer backend.
pub struct AppConfig {
    pub window: WindowConfig,
    /// The faces this application ships, embedded in the binary or read from a file beside it. The faces `[[telar.fonts]]` declares in `telar.toml` are added by `telar::app!`; see `docs/fonts.md`.
    pub fonts: Vec<FontAsset>,
    /// The family this application's unstyled text shapes in — a shell's theme font. `None` keeps the platform's own. Loading a face with `fonts` does not choose it; this does.
    ///
    /// A property of *this* configuration, so a second surface built later renders in its own family rather than in whichever one was configured last. A single text overrides it with [`TextStyle::with_font_family`](crate::TextStyle::with_font_family).
    pub font_family: Option<String>,
}

impl AppConfig {
    /// This configuration with `fonts` added: what `telar::app!` calls with the faces `telar.toml` declares.
    pub fn with_fonts(mut self, fonts: impl IntoIterator<Item = FontAsset>) -> Self {
        self.fonts.extend(fonts);
        self
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
