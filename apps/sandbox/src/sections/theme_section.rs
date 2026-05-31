use crate::theme::{SandboxTheme, heading};
use rsx::set_theme;
use rsx::{
    BorderRadius, Button, ButtonStyle, Color, Container, LayoutError, LayoutItem, LayoutStyle,
    RectStyle, Text, TextStyle, WidgetCtx, use_theme,
};

pub fn theme_section(ctx: &mut WidgetCtx) -> Result<Container, LayoutError> {
    let h = heading(ctx, "Theme")?;

    let btn_modern = Button::new(ctx, "Modern")?.on_click(|| set_theme(SandboxTheme::modern()));

    let btn_pastel = Button::new(ctx, "Pastel")?
        .style(|| ButtonStyle {
            rect: RectStyle::default()
                .with_fill(Color::rgba(0.82, 0.72, 0.94, 1.0))
                .with_radius(BorderRadius::all(4.0)),
            rect_hover: RectStyle::default()
                .with_fill(Color::rgba(0.74, 0.63, 0.88, 1.0))
                .with_radius(BorderRadius::all(4.0)),
            text: TextStyle::new(14.0, Color::rgba(0.25, 0.15, 0.35, 1.0)),
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

    Container::new(
        ctx,
        LayoutStyle::new().flex_column().gap(8.0),
        vec![
            h,
            Box::new(btn_row) as Box<dyn LayoutItem>,
            Box::new(status) as Box<dyn LayoutItem>,
        ],
    )
}
