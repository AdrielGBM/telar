use std::sync::Arc;

use rsx::{
    Container, Image, ImageData, ImageFilter, LayoutError, LayoutStyle, Text, TextStyle, WidgetCtx,
    children,
};

use crate::theme::{section, theme};

pub fn image_with_label(
    ctx: &mut WidgetCtx,
    data: Arc<ImageData>,
    filter: ImageFilter,
    size: f32,
    label: &'static str,
) -> Result<Container, LayoutError> {
    let image = Image::new(
        ctx,
        {
            let d = data.clone();
            move || d.clone()
        },
        LayoutStyle::new().width(size).height(size),
        move || filter,
    )?;
    let label_widget = Text::new(
        ctx,
        move || label.to_string(),
        LayoutStyle::new().width(size).height(16.0),
        || TextStyle::new(11.0, theme().muted),
    )?;
    Container::new(
        ctx,
        LayoutStyle::new().flex_column().gap(4.0),
        children![image, label_widget],
    )
}

pub fn images_section(
    ctx: &mut WidgetCtx,
    gradient: Arc<ImageData>,
    checker: Arc<ImageData>,
    alpha: Arc<ImageData>,
) -> Result<Container, LayoutError> {
    let i1 = image_with_label(ctx, gradient, ImageFilter::Linear, 128.0, "gradient")?;
    let i2 = image_with_label(
        ctx,
        checker,
        ImageFilter::Nearest,
        192.0,
        "checker (scaled)",
    )?;
    let i3 = image_with_label(ctx, alpha, ImageFilter::Nearest, 128.0, "alpha blend")?;
    let row = Container::new(
        ctx,
        LayoutStyle::new().flex_row().gap(20.0),
        children![i1, i2, i3],
    )?;
    section(ctx, "Images", row)
}
