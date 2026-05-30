use crate::theme::{SandboxTheme, heading};
use rsx::{
    BorderRadius, Color, Container, DrawCommand, DrawingArea, LayoutError, LayoutItem, LayoutStyle,
    Paint, Rect, RectPayload, RectStyle, TextPayload, TextStyle, View, WidgetCtx, use_theme,
};
use std::rc::Rc;

pub fn color_swatch(
    ctx: &mut WidgetCtx,
    color_fn: impl Fn() -> Color + 'static,
    label: &'static str,
) -> Result<DrawingArea, LayoutError> {
    DrawingArea::new(
        ctx,
        LayoutStyle::new().width(100.0).height(44.0),
        move |_w, _h| {
            View::group([
                View::Primitive(DrawCommand::Rect(Box::new(RectPayload {
                    rect: Rect {
                        x: 0.0,
                        y: 0.0,
                        width: 100.0,
                        height: 44.0,
                    },
                    style: RectStyle {
                        fill: Some(Paint::Solid(color_fn())),
                        stroke: None,
                        shadow: None,
                        radius: BorderRadius::all(6.0),
                    },
                }))),
                View::Primitive(DrawCommand::Text(Box::new(TextPayload {
                    text: Rc::from(label),
                    rect: Rect {
                        x: 0.0,
                        y: 4.0,
                        width: 100.0,
                        height: 36.0,
                    },
                    style: TextStyle::new(11.0, use_theme::<SandboxTheme>().on_color),
                }))),
            ])
        },
    )
}

pub fn colors_section(ctx: &mut WidgetCtx) -> Result<Container, LayoutError> {
    type ColorAccessor = fn(&SandboxTheme) -> Color;
    let swatches: [(ColorAccessor, &'static str); 6] = [
        (|t| t.primary, "primary"),
        (|t| t.success, "success"),
        (|t| t.danger, "danger"),
        (|t| t.warning, "warning"),
        (|t| t.purple, "purple"),
        (|t| t.dark, "dark"),
    ];

    let mut row_children: Vec<Box<dyn LayoutItem>> = Vec::new();
    for (accessor, label) in swatches {
        row_children.push(Box::new(color_swatch(
            ctx,
            move || accessor(&use_theme::<SandboxTheme>()),
            label,
        )?) as Box<dyn LayoutItem>);
    }

    let row = Container::new(ctx, LayoutStyle::new().flex_row().gap(16.0), row_children)?;
    let h = heading(ctx, "Colors")?;
    Container::new(
        ctx,
        LayoutStyle::new().flex_column().gap(8.0),
        vec![h, Box::new(row) as Box<dyn LayoutItem>],
    )
}
