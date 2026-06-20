use crate::theme::{SandboxTheme, section, theme};
use rsx::{
    BorderRadius, Canvas, Color, Container, LayoutError, LayoutStyle, Rect, RectStyle, RenderNode,
    TextStyle, WidgetCtx, box_item,
};

pub fn color_swatch(
    ctx: &mut WidgetCtx,
    color_fn: impl Fn() -> Color + 'static,
    label: &'static str,
) -> Result<Canvas, LayoutError> {
    Canvas::new(
        ctx,
        LayoutStyle::new().width(100.0).height(44.0),
        move |_| {
            RenderNode::group([
                RenderNode::rect(
                    Rect {
                        x: 0.0,
                        y: 0.0,
                        width: 100.0,
                        height: 44.0,
                    },
                    RectStyle::filled(color_fn(), BorderRadius::all(6.0)),
                ),
                RenderNode::text(
                    label,
                    Rect {
                        x: 0.0,
                        y: 4.0,
                        width: 100.0,
                        height: 36.0,
                    },
                    TextStyle::new(11.0, theme().on_color),
                ),
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

    let mut row_children = Vec::new();
    for (accessor, label) in swatches {
        row_children.push(box_item(color_swatch(
            ctx,
            move || accessor(&theme()),
            label,
        )?));
    }

    let row = Container::new(ctx, LayoutStyle::new().flex_row().gap(16.0), row_children)?;
    section(ctx, "Colors", row)
}
