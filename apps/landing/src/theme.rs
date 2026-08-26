use telar::{Color, ThemeTokens, use_theme};

// A one-page site with no controls to speak of: the metrics and the status hues stay the catalogue's, said
// here rather than left to silence. `surface`/`surface_alt`/`border`/`success` now reach the catalogue, which
// the hand-written impl never forwarded despite the fields existing.
#[derive(Clone, ThemeTokens)]
#[theme(default(
    radius,
    spacing,
    icon_size,
    scrollbar,
    ink,
    warning,
    error,
    info,
    highlight_low,
    highlight_med,
    highlight_high
))]
pub struct LandingTheme {
    pub background: Color,
    pub surface: Color,
    pub surface_alt: Color,
    pub border: Color,
    pub primary: Color,
    pub accent: Color,
    pub success: Color,
    pub dark: Color,
    pub muted: Color,
    pub on_primary: Color,
    pub on_dark: Color,
}

impl LandingTheme {
    pub fn light() -> Self {
        Self {
            background: Color::rgba(0.98, 0.98, 1.0, 1.0),
            surface: Color::WHITE,
            surface_alt: Color::rgba(0.95, 0.96, 0.99, 1.0),
            border: Color::rgba(0.86, 0.87, 0.93, 1.0),
            primary: Color::rgba(0.26, 0.38, 0.93, 1.0),
            accent: Color::rgba(0.93, 0.33, 0.55, 1.0),
            success: Color::rgba(0.18, 0.69, 0.45, 1.0),
            dark: Color::rgba(0.09, 0.10, 0.18, 1.0),
            muted: Color::rgba(0.46, 0.48, 0.58, 1.0),
            on_primary: Color::WHITE,
            on_dark: Color::rgba(0.78, 0.80, 0.88, 1.0),
        }
    }
}

pub fn theme() -> LandingTheme {
    use_theme::<LandingTheme>()
}
