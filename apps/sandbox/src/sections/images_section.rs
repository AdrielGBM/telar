use std::rc::Rc;

use rsx::{
    Container, Image, ImageData, ImageFilter, LayoutError, LayoutItem, LayoutStyle, Text,
    TextStyle, WidgetCtx, use_theme,
};

use crate::theme::{SandboxTheme, heading};

pub fn image_with_label(
    ctx: &mut WidgetCtx,
    data: Rc<ImageData>,
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
    let label = Text::new(
        ctx,
        move || label.to_string(),
        LayoutStyle::new().width(size).height(16.0),
        || TextStyle::new(11.0, use_theme::<SandboxTheme>().muted),
    )?;

    Container::new(
        ctx,
        LayoutStyle::new().flex_column().gap(4.0),
        vec![
            Box::new(image) as Box<dyn LayoutItem>,
            Box::new(label) as Box<dyn LayoutItem>,
        ],
    )
}

pub fn images_section(
    ctx: &mut WidgetCtx,
    gradient: Rc<ImageData>,
    checker: Rc<ImageData>,
    alpha: Rc<ImageData>,
) -> Result<Container, LayoutError> {
    let i1 = Box::new(image_with_label(
        ctx,
        gradient,
        ImageFilter::Linear,
        128.0,
        "gradient",
    )?) as Box<dyn LayoutItem>;
    let i2 = Box::new(image_with_label(
        ctx,
        checker,
        ImageFilter::Nearest,
        192.0,
        "checker (scaled)",
    )?) as Box<dyn LayoutItem>;
    let i3 = Box::new(image_with_label(
        ctx,
        alpha,
        ImageFilter::Nearest,
        128.0,
        "alpha blend",
    )?) as Box<dyn LayoutItem>;
    let row = Container::new(
        ctx,
        LayoutStyle::new().flex_row().gap(20.0),
        vec![i1, i2, i3],
    )?;
    let h = heading(ctx, "Images")?;
    Container::new(
        ctx,
        LayoutStyle::new().flex_column().gap(8.0),
        vec![h, Box::new(row) as Box<dyn LayoutItem>],
    )
}
