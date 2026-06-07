use crate::theme::SandboxTheme;
use crate::theme::section;
use rsx::{
    Color, Container, LayoutError, LayoutItem, LayoutStyle, Text, TextStyle, WidgetCtx, use_theme,
};

pub fn type_line(
    ctx: &mut WidgetCtx,
    label: &'static str,
    size: f32,
    color_fn: impl Fn() -> Color + 'static,
    height: f32,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let text = Text::new(
        ctx,
        move || label.to_string(),
        LayoutStyle::new().height(height),
        move || TextStyle::new(size, color_fn()),
    )?;
    Ok(Box::new(text) as Box<dyn LayoutItem>)
}

pub fn typography_section(ctx: &mut WidgetCtx) -> Result<Container, LayoutError> {
    let t1 = type_line(
        ctx,
        "Small — 12px — The quick brown fox",
        12.0,
        || use_theme::<SandboxTheme>().dark,
        20.0,
    )?;
    let t2 = type_line(
        ctx,
        "Regular — 14px — The quick brown fox",
        14.0,
        || use_theme::<SandboxTheme>().dark,
        22.0,
    )?;
    let t3 = type_line(
        ctx,
        "Medium — 18px — The quick brown fox",
        18.0,
        || use_theme::<SandboxTheme>().dark,
        26.0,
    )?;
    let t4 = type_line(
        ctx,
        "Large — 24px — The quick brown fox",
        24.0,
        || use_theme::<SandboxTheme>().dark,
        32.0,
    )?;
    let t5 = type_line(
        ctx,
        "Display — 32px",
        32.0,
        || use_theme::<SandboxTheme>().primary,
        42.0,
    )?;
    let content = Container::new(
        ctx,
        LayoutStyle::new().flex_column().gap(8.0),
        vec![t1, t2, t3, t4, t5],
    )?;
    section(ctx, "Typography", content)
}
