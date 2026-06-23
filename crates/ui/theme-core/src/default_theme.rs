use renderer_core::Color;

use crate::context::{Theme, WidgetTheme};

/// A ready-to-use light theme so apps (and `theme-core`) have a concrete theme
/// out of the box instead of only the `Theme`/`WidgetTheme` traits. Construct
/// with `DefaultTheme::light()` (also `Default`), tweak fields, then install via
/// `set_theme_with_widgets`.
#[derive(Clone)]
pub struct DefaultTheme {
    pub primary: Color,
    pub on_primary: Color,
    pub surface: Color,
    pub on_surface: Color,
    pub muted: Color,
    pub border: Color,
    pub danger: Color,
    pub success: Color,
    pub warning: Color,
    pub accent: Color,
    pub purple: Color,
    pub scrollbar: Color,
}

impl DefaultTheme {
    pub fn light() -> Self {
        Self {
            primary: Color::rgba(0.24, 0.47, 0.98, 1.0),
            on_primary: Color::rgba(1.0, 1.0, 1.0, 1.0),
            surface: Color::rgba(0.98, 0.98, 0.99, 1.0),
            on_surface: Color::rgba(0.08, 0.08, 0.14, 1.0),
            muted: Color::rgba(0.5, 0.5, 0.6, 1.0),
            border: Color::rgba(0.8, 0.8, 0.88, 1.0),
            danger: Color::rgba(0.92, 0.27, 0.27, 1.0),
            success: Color::rgba(0.18, 0.69, 0.45, 1.0),
            warning: Color::rgba(0.95, 0.72, 0.18, 1.0),
            accent: Color::rgba(0.2, 0.75, 0.9, 1.0),
            purple: Color::rgba(0.6, 0.28, 0.98, 1.0),
            scrollbar: Color::rgba(0.5, 0.5, 0.6, 0.6),
        }
    }
}

impl Default for DefaultTheme {
    fn default() -> Self {
        Self::light()
    }
}

impl Theme for DefaultTheme {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl WidgetTheme for DefaultTheme {
    fn widget_primary(&self) -> Color {
        self.primary
    }
    fn widget_on_primary(&self) -> Color {
        self.on_primary
    }
    fn widget_surface(&self) -> Color {
        self.surface
    }
    fn widget_on_surface(&self) -> Color {
        self.on_surface
    }
    fn widget_scrollbar(&self) -> Color {
        self.scrollbar
    }
    fn widget_danger(&self) -> Color {
        self.danger
    }
    fn widget_success(&self) -> Color {
        self.success
    }
    fn widget_muted(&self) -> Color {
        self.muted
    }
    fn widget_warning(&self) -> Color {
        self.warning
    }
    fn widget_accent(&self) -> Color {
        self.accent
    }
    fn widget_border(&self) -> Color {
        self.border
    }
    fn widget_purple(&self) -> Color {
        self.purple
    }
}
