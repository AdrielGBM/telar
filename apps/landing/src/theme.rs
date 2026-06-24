use rsx::{Color, Theme, WidgetTheme, use_theme};

#[derive(Clone)]
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

impl Theme for LandingTheme {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl WidgetTheme for LandingTheme {
    fn widget_primary(&self) -> Color {
        self.primary
    }
    fn widget_on_primary(&self) -> Color {
        self.on_primary
    }
    fn widget_muted(&self) -> Color {
        self.muted
    }
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
