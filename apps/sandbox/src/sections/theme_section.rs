use crate::theme::SandboxTheme;
use crate::theme::section;
use rsx::set_theme;
use rsx::{
    BorderRadius, Button, ButtonStyle, Color, Container, LayoutError, LayoutItem, LayoutStyle,
    RectStyle, Text, TextStyle, WidgetCtx, use_theme,
};

pub fn theme_section(ctx: &mut WidgetCtx) -> Result<Container, LayoutError> {
    let btn_modern = Button::new(ctx, "Modern")?
        .style(|| {
            let theme = use_theme::<SandboxTheme>();
            let p = theme.primary;
            let hover = Color::rgba(
                (p.r * 0.85).min(1.0),
                (p.g * 0.85).min(1.0),
                (p.b * 0.85).min(1.0),
                1.0,
            );
            ButtonStyle {
                rect: RectStyle::default()
                    .with_fill(theme.primary)
                    .with_radius(BorderRadius::all(4.0)),
                rect_hover: RectStyle::default()
                    .with_fill(hover)
                    .with_radius(BorderRadius::all(4.0)),
                text: TextStyle::new(14.0, theme.on_color),
                text_hover: TextStyle::new(14.0, theme.on_color),
            }
        })
        .on_click(|| set_theme(SandboxTheme::modern()));

    let btn_pastel = Button::new(ctx, "Pastel")?
        .style(|| {
            let p = SandboxTheme::pastel();
            let hover = Color::rgba(
                (p.primary.r * 0.85).min(1.0),
                (p.primary.g * 0.85).min(1.0),
                (p.primary.b * 0.85).min(1.0),
                1.0,
            );
            ButtonStyle {
                rect: RectStyle::default()
                    .with_fill(p.primary)
                    .with_radius(BorderRadius::all(4.0)),
                rect_hover: RectStyle::default()
                    .with_fill(hover)
                    .with_radius(BorderRadius::all(4.0)),
                text: TextStyle::new(14.0, p.on_color),
                text_hover: TextStyle::new(14.0, p.on_color),
            }
        })
        .on_click(|| set_theme(SandboxTheme::pastel()));

    let btn_row = Container::new(
        ctx,
        LayoutStyle::new().flex_row().gap(8.0),
        vec![
            Box::new(btn_modern) as Box<dyn LayoutItem>,
            Box::new(btn_pastel) as Box<dyn LayoutItem>,
        ],
    )?;

    let status = Text::new(
        ctx,
        || format!("Active: {}", use_theme::<SandboxTheme>().name),
        LayoutStyle::new().height(20.0),
        || TextStyle::new(13.0, use_theme::<SandboxTheme>().muted),
    )?;

    let content = Container::new(
        ctx,
        LayoutStyle::new().flex_column().gap(8.0),
        vec![
            Box::new(btn_row) as Box<dyn LayoutItem>,
            Box::new(status) as Box<dyn LayoutItem>,
        ],
    )?;
    section(ctx, "Theme", content)
}
