//! The configuration an application starts with: its window, its fonts, its renderer and the URI schemes it goes to.

use platform_core::{UriSchemes, WindowConfig};
use renderer_core::{FontAsset, FontFamily};

#[derive(Clone, Default)]
/// What an application starts with: its window, its fonts and its renderer backend.
pub struct AppConfig {
    /// The window the application opens.
    pub window: WindowConfig,
    /// The faces this application ships, embedded in the binary or read from a file beside it. The faces `[[telar.fonts]]` declares in `telar.toml` are added by `telar::app!`; see `docs/fonts.md`.
    pub fonts: Vec<FontAsset>,
    /// The family this surface's text shapes in where nothing above it names one — a shell's theme font. `None` keeps the platform's own. Loading a face with `fonts` does not choose it; this does.
    ///
    /// It seeds the root of the surface's text cascade as the surface opens, and [`set_font_family`](crate::set_font_family) moves it while the surface runs. A property of *this* configuration, so a second surface built later renders in its own family rather than in whichever one was configured last. Text declares its own with `font_family:` or [`TextStyle::with_font_family`](crate::TextStyle::with_font_family); a text given a whole style with [`Text::new`](crate::Text::new) opts out of the cascade, and with it out of this.
    pub font_family: Option<FontFamily>,
    /// The schemes this surface's external links go to and its links open: [`DEFAULT_URI_SCHEMES`](crate::DEFAULT_URI_SCHEMES) unless the app adds its own with [`UriSchemes::allowing`]. Handed to the surface as it opens, a guest's included, so a link is judged by them whichever side of a hot reload built it; see `docs/links.md`.
    pub uri_schemes: UriSchemes,
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
