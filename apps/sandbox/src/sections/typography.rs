use crate::theme::{section, theme};
use rsx::{Color, Container, LayoutError, LayoutItem, LayoutStyle, Text, TextStyle, WidgetCtx};

pub fn type_line(
    ctx: &mut WidgetCtx,
    label: &'static str,
    size: f32,
    color_fn: impl Fn() -> Color + 'static,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let text = Text::single_line(
        ctx,
        move || label.to_string(),
        move || TextStyle::new(size, color_fn()),
    )?;
    Ok(rsx::box_item(text))
}

pub fn typography_section(ctx: &mut WidgetCtx) -> Result<Container, LayoutError> {
    let t1 = type_line(ctx, "Small — 12px — The quick brown fox", 12.0, || {
        theme().dark
    })?;
    let t2 = type_line(
        ctx,
        "Regular — 14px — The quick brown fox",
        14.0,
        || theme().dark,
    )?;
    let t3 = type_line(ctx, "Medium — 18px — The quick brown fox", 18.0, || {
        theme().dark
    })?;
    let t4 = type_line(ctx, "Large — 24px — The quick brown fox", 24.0, || {
        theme().dark
    })?;
    let t5 = type_line(ctx, "Display — 32px", 32.0, || theme().primary)?;
    let content = Container::new(
        ctx,
        LayoutStyle::new().flex_column().gap(8.0),
        vec![t1, t2, t3, t4, t5],
    )?;
    section(ctx, "Typography", content)
}
