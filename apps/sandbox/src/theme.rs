use rsx::{Color, LayoutError, LayoutItem, LayoutStyle, Text, TextStyle, WidgetCtx, use_theme};

pub struct SandboxTheme {
    pub name: &'static str,
    pub background: Color,
    pub primary: Color,
    pub success: Color,
    pub danger: Color,
    pub warning: Color,
    pub purple: Color,
    pub dark: Color,
    pub muted: Color,
    pub card_border: Color,
    pub cyan: Color,
    pub on_color: Color,
}

impl SandboxTheme {
    pub fn modern() -> Self {
        Self {
            name: "Modern",
            background: Color::rgba(0.97, 0.97, 0.99, 1.0),
            primary: Color::rgba(0.24, 0.47, 0.98, 1.0),
            success: Color::rgba(0.18, 0.69, 0.45, 1.0),
            danger: Color::rgba(0.92, 0.27, 0.27, 1.0),
            warning: Color::rgba(0.97, 0.72, 0.18, 1.0),
            purple: Color::rgba(0.60, 0.28, 0.98, 1.0),
            dark: Color::rgba(0.08, 0.08, 0.14, 1.0),
            muted: Color::rgba(0.50, 0.50, 0.60, 1.0),
            card_border: Color::rgba(0.80, 0.80, 0.88, 1.0),
            cyan: Color::rgba(0.20, 0.75, 0.90, 1.0),
            on_color: Color::WHITE,
        }
    }

    pub fn pastel() -> Self {
        Self {
            name: "Pastel",
            background: Color::rgba(0.96, 0.92, 0.98, 1.0),
            primary: Color::rgba(0.61, 0.74, 0.98, 1.0),
            success: Color::rgba(0.58, 0.88, 0.73, 1.0),
            danger: Color::rgba(0.98, 0.67, 0.67, 1.0),
            warning: Color::rgba(0.99, 0.88, 0.58, 1.0),
            purple: Color::rgba(0.82, 0.70, 0.98, 1.0),
            dark: Color::rgba(0.30, 0.25, 0.38, 1.0),
            muted: Color::rgba(0.58, 0.48, 0.65, 1.0),
            card_border: Color::rgba(0.88, 0.83, 0.92, 1.0),
            cyan: Color::rgba(0.65, 0.92, 0.90, 1.0),
            on_color: Color::rgba(0.15, 0.10, 0.25, 1.0),
        }
    }
}

pub fn heading(
    ctx: &mut WidgetCtx,
    label: &'static str,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let text = Text::new(
        ctx,
        move || label.to_string(),
        LayoutStyle::new().height(20.0),
        || TextStyle::new(12.0, use_theme::<SandboxTheme>().muted),
    )?;
    Ok(Box::new(text) as Box<dyn LayoutItem>)
}
