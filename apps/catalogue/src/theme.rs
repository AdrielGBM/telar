use telar::{Color, ThemeTokens, use_theme};

/// The application's design tokens. Every component reads these, so a restyle happens here and nowhere else.
#[derive(Clone, ThemeTokens)]
pub struct AppTheme {
    pub primary: Color,
    pub on_primary: Color,
    pub surface: Color,
    pub surface_alt: Color,
    pub border: Color,
    pub ink: Color,
    pub muted: Color,
    pub scrollbar: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub info: Color,
    pub highlight_low: Color,
    pub highlight_med: Color,
    pub highlight_high: Color,
    pub radius: f32,
    pub spacing: f32,
    pub icon_size: f32,
}

impl AppTheme {
    pub fn light() -> Self {
        Self {
            primary: Color::rgba(0.26, 0.38, 0.93, 1.0),
            on_primary: Color::WHITE,
            surface: Color::WHITE,
            surface_alt: Color::rgba(0.96, 0.97, 0.99, 1.0),
            border: Color::rgba(0.86, 0.87, 0.93, 1.0),
            ink: Color::rgba(0.09, 0.10, 0.18, 1.0),
            muted: Color::rgba(0.46, 0.48, 0.58, 1.0),
            scrollbar: Color::rgba(0.66, 0.68, 0.76, 1.0),
            success: Color::rgba(0.18, 0.69, 0.45, 1.0),
            warning: Color::rgba(0.90, 0.62, 0.16, 1.0),
            error: Color::rgba(0.86, 0.26, 0.30, 1.0),
            info: Color::rgba(0.24, 0.55, 0.90, 1.0),
            highlight_low: Color::rgba(0.0, 0.0, 0.0, 0.04),
            highlight_med: Color::rgba(0.0, 0.0, 0.0, 0.08),
            highlight_high: Color::rgba(0.0, 0.0, 0.0, 0.14),
            radius: 10.0,
            spacing: 8.0,
            icon_size: 16.0,
        }
    }

    pub fn dark() -> Self {
        Self {
            primary: Color::rgba(0.45, 0.58, 1.0, 1.0),
            on_primary: Color::rgba(0.05, 0.06, 0.12, 1.0),
            surface: Color::rgba(0.11, 0.12, 0.16, 1.0),
            surface_alt: Color::rgba(0.07, 0.08, 0.11, 1.0),
            border: Color::rgba(0.24, 0.26, 0.32, 1.0),
            ink: Color::rgba(0.92, 0.93, 0.96, 1.0),
            muted: Color::rgba(0.60, 0.63, 0.72, 1.0),
            scrollbar: Color::rgba(0.36, 0.38, 0.46, 1.0),
            success: Color::rgba(0.30, 0.78, 0.55, 1.0),
            warning: Color::rgba(0.96, 0.72, 0.28, 1.0),
            error: Color::rgba(0.94, 0.42, 0.44, 1.0),
            info: Color::rgba(0.42, 0.68, 0.98, 1.0),
            highlight_low: Color::rgba(1.0, 1.0, 1.0, 0.05),
            highlight_med: Color::rgba(1.0, 1.0, 1.0, 0.10),
            highlight_high: Color::rgba(1.0, 1.0, 1.0, 0.16),
            radius: 10.0,
            spacing: 8.0,
            icon_size: 16.0,
        }
    }
}

/// The active theme, read reactively: a component calling this re-runs when the theme changes.
pub fn theme() -> AppTheme {
    use_theme::<AppTheme>()
}
