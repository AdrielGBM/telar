use crate::theme::{SandboxTheme, section, theme};
use rsx::set_theme_with_widgets;
use rsx::{
    BorderRadius, Button, ButtonStyle, Container, LayoutError, LayoutStyle, RectStyle, ShapeStyle,
    Text, TextStyle, WidgetCtx, children,
};

pub fn theme_section(ctx: &mut WidgetCtx) -> Result<Container, LayoutError> {
    let btn_modern = Button::new(ctx, "Modern")?
        .style(|| {
            let t = theme();
            let hover = t.primary.darken(0.15);
            ButtonStyle {
                rect: RectStyle::default()
                    .with_fill(t.primary)
                    .with_radius(BorderRadius::all(4.0)),
                rect_hover: RectStyle::default()
                    .with_fill(hover)
                    .with_radius(BorderRadius::all(4.0)),
                text: TextStyle::new(14.0, t.on_color),
                text_hover: TextStyle::new(14.0, t.on_color),
            }
        })
        .on_click(|| set_theme_with_widgets(SandboxTheme::modern()));

    let btn_pastel = Button::new(ctx, "Pastel")?
        .style(|| {
            let p = SandboxTheme::pastel();
            let hover = p.primary.darken(0.15);
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
        .on_click(|| set_theme_with_widgets(SandboxTheme::pastel()));

    let btn_row = Container::new(
        ctx,
        LayoutStyle::new().flex_row().gap(8.0),
        children![btn_modern, btn_pastel],
    )?;

    let status = Text::new(
        ctx,
        || format!("Active: {}", theme().name),
        LayoutStyle::new().height(20.0),
        || TextStyle::new(13.0, theme().muted),
    )?;

    let content = Container::new(
        ctx,
        LayoutStyle::new().flex_column().gap(8.0),
        children![btn_row, status],
    )?;
    section(ctx, "Theme", content)
}
