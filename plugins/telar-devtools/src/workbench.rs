//! The workbench theme: the one look the workshop and the overlay share, light and dark, applied to a subtree without touching the application's own theme.

use telar::{
    Border, BorderRadius, Color, ColorScheme, ControlSize, Declared, FontFamily, LayoutError,
    LayoutItem, LayoutStyle, RectStyle, ScopedTheme, TextStyle, ThemeProvider, ThemeTokens,
    follow_theme, provide_theme, use_resolved_scheme, use_theme_extension,
};

pub const WORKBENCH_TEXT_SIZE: f32 = 13.0;
pub const WORKBENCH_RADIUS: f32 = 6.0;
pub const WORKBENCH_GRID: f32 = 8.0;
pub const WORKBENCH_CONTROL_SIZE: ControlSize = ControlSize::Regular;

const fn hex(rgb: u32) -> Color {
    Color::rgba(
        ((rgb >> 16) & 0xff) as f32 / 255.0,
        ((rgb >> 8) & 0xff) as f32 / 255.0,
        (rgb & 0xff) as f32 / 255.0,
        1.0,
    )
}

const fn neutral(alpha: f32) -> Color {
    Color::rgba(0.5, 0.52, 0.56, alpha)
}

/// Values only the workbench's own chrome reads, through [`use_workbench_tokens`].
#[derive(Clone, Debug, PartialEq)]
pub struct WorkbenchTokens {
    pub panel_background: Color,
    pub sidebar_background: Color,
    pub border_subtle: Color,
    pub canvas_backdrop: Color,
    pub checker_a: Color,
    pub checker_b: Color,
    pub selection: Color,
    pub text_muted: Color,
    pub mono_size: f32,
    pub mono_family: FontFamily,
}

impl WorkbenchTokens {
    pub fn light() -> Self {
        Self {
            panel_background: hex(0xFFFFFF),
            sidebar_background: hex(0xF4F5F7),
            border_subtle: hex(0xE3E6EA),
            canvas_backdrop: hex(0xECEEF1),
            checker_a: hex(0xFFFFFF),
            checker_b: hex(0xE4E6EA),
            selection: Color::rgba(0.133, 0.345, 0.784, 0.14),
            text_muted: hex(0x586170),
            mono_size: 12.0,
            mono_family: FontFamily::Monospace,
        }
    }

    pub fn dark() -> Self {
        Self {
            panel_background: hex(0x1A1C20),
            sidebar_background: hex(0x141619),
            border_subtle: hex(0x2A2D33),
            canvas_backdrop: hex(0x0F1113),
            checker_a: hex(0x33363B),
            checker_b: hex(0x2A2D31),
            selection: Color::rgba(0.478, 0.635, 1.0, 0.18),
            text_muted: hex(0x9BA3AF),
            mono_size: 12.0,
            mono_family: FontFamily::Monospace,
        }
    }
}

impl Default for WorkbenchTokens {
    fn default() -> Self {
        Self::light()
    }
}

/// The workbench palette as a theme: every shared token the catalogue reads, plus [`WorkbenchTokens`].
#[derive(Clone, ThemeTokens)]
#[theme(root = Declared::default().with_font_size(WORKBENCH_TEXT_SIZE).with_color(self.ink))]
pub struct WorkbenchTheme {
    pub primary: Color,
    pub on_primary: Color,
    pub radius: f32,
    pub spacing: f32,
    pub icon_size: f32,
    pub muted: Color,
    pub scrollbar: Color,
    pub ink: Color,
    pub surface: Color,
    pub surface_alt: Color,
    pub border: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub info: Color,
    pub highlight_low: Color,
    pub highlight_med: Color,
    pub highlight_high: Color,
    #[theme(extension)]
    pub workbench: WorkbenchTokens,
}

impl WorkbenchTheme {
    pub fn light() -> Self {
        let workbench = WorkbenchTokens::light();
        Self {
            primary: hex(0x2257C8),
            on_primary: hex(0xFFFFFF),
            radius: WORKBENCH_RADIUS,
            spacing: WORKBENCH_GRID,
            icon_size: 16.0,
            muted: workbench.text_muted,
            scrollbar: neutral(0.45),
            ink: hex(0x1B1F24),
            surface: workbench.panel_background,
            surface_alt: hex(0xEEF0F3),
            border: hex(0xD8DCE1),
            success: hex(0x1A7F37),
            warning: hex(0x8A5A00),
            error: hex(0xC0262D),
            info: hex(0x1F62A8),
            highlight_low: neutral(0.07),
            highlight_med: neutral(0.13),
            highlight_high: neutral(0.22),
            workbench,
        }
    }

    pub fn dark() -> Self {
        let workbench = WorkbenchTokens::dark();
        Self {
            primary: hex(0x7AA2FF),
            on_primary: hex(0x0B1220),
            radius: WORKBENCH_RADIUS,
            spacing: WORKBENCH_GRID,
            icon_size: 16.0,
            muted: workbench.text_muted,
            scrollbar: neutral(0.45),
            ink: hex(0xE6E8EB),
            surface: workbench.panel_background,
            surface_alt: hex(0x24272C),
            border: hex(0x33373D),
            success: hex(0x4CC38A),
            warning: hex(0xE3B341),
            error: hex(0xF47067),
            info: hex(0x6CB6FF),
            highlight_low: neutral(0.10),
            highlight_med: neutral(0.18),
            highlight_high: neutral(0.28),
            workbench,
        }
    }

    pub fn for_scheme(scheme: ColorScheme) -> Self {
        match scheme {
            ColorScheme::Dark => Self::dark(),
            _ => Self::light(),
        }
    }
}

/// The workbench tokens in force here, or the light ones outside a workbench scope.
pub fn use_workbench_tokens() -> WorkbenchTokens {
    use_theme_extension::<WorkbenchTokens>()
}

/// A scoped theme that re-resolves to the light or dark workbench whenever the resolved system scheme changes.
pub fn workbench_theme() -> ScopedTheme {
    follow_theme(|| WorkbenchTheme::for_scheme(use_resolved_scheme()))
}

/// Builds `child` under the workbench theme, leaving the application's global theme alone.
pub fn workbench_scope(
    child: impl FnOnce() -> Result<Box<dyn LayoutItem>, LayoutError>,
) -> Result<ThemeProvider, LayoutError> {
    provide_theme(workbench_theme(), child)
}

/// The outline every workbench card, field and frame shares: a one-pixel subtle border on the workbench radius. The caller adds the fill, if any.
pub fn workbench_card() -> RectStyle {
    RectStyle::default()
        .with_border(Border::uniform(use_workbench_tokens().border_subtle, 1.0))
        .with_radius(BorderRadius::all(WORKBENCH_RADIUS))
}

/// Text that steps back: hints, empty states and secondary details.
pub fn workbench_muted(text: TextStyle) -> TextStyle {
    text.with_color(use_workbench_tokens().text_muted)
}

/// Code, values and measurements, in the workbench's monospace face and size.
pub fn workbench_mono(text: TextStyle) -> TextStyle {
    let tokens = use_workbench_tokens();
    text.with_font_family(tokens.mono_family)
        .with_font_size(tokens.mono_size)
}

/// A column that takes the space its parent leaves and may shrink below its content either way, so a scroll area or a list inside it bounds its own overflow.
pub fn workbench_fill() -> LayoutStyle {
    LayoutStyle::new()
        .flex_column()
        .flex_grow(1.0)
        .min_width(0.0)
        .min_height(0.0)
}

#[cfg(test)]
#[path = "workbench_test.rs"]
mod tests;
