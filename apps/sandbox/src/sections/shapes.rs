use crate::theme::{section, theme};
use rsx::{
    BorderRadius, Canvas, Color, Container, LayoutError, LayoutStyle, Paint, Rect, RectStyle,
    RenderNode, Stroke, TextStyle, WidgetCtx, children,
};

pub fn shape_card(
    ctx: &mut WidgetCtx,
    style_fn: impl Fn() -> RectStyle + 'static,
    label: &'static str,
    label_color_fn: impl Fn() -> Color + 'static,
) -> Result<Canvas, LayoutError> {
    Canvas::new(
        ctx,
        LayoutStyle::new().width(168.0).height(80.0),
        move |rect| {
            RenderNode::group([
                RenderNode::rect(
                    Rect {
                        x: 0.0,
                        y: 0.0,
                        width: rect.width,
                        height: rect.height,
                    },
                    style_fn(),
                ),
                RenderNode::text(
                    label,
                    Rect {
                        x: 0.0,
                        y: 4.0,
                        width: rect.width,
                        height: rect.height - 8.0,
                    },
                    TextStyle::new(13.0, label_color_fn()),
                ),
            ])
        },
    )
}

pub fn shapes_section(ctx: &mut WidgetCtx) -> Result<Container, LayoutError> {
    let sc1 = shape_card(
        ctx,
        || RectStyle::filled(theme().primary, BorderRadius::all(8.0)),
        "fill",
        || theme().on_color,
    )?;
    let sc2 = shape_card(
        ctx,
        || RectStyle {
            fill: None,
            stroke: Some(Stroke::new(theme().danger, 2.0)),
            shadow: None,
            radius: BorderRadius::all(8.0),
        },
        "stroke",
        || theme().danger,
    )?;
    let sc3 = shape_card(
        ctx,
        || {
            let t = theme();
            RectStyle {
                fill: Some(Paint::Solid(t.success)),
                stroke: Some(Stroke::new(t.dark, 1.5)),
                shadow: None,
                radius: BorderRadius::zero(),
            }
        },
        "fill + stroke",
        || theme().on_color,
    )?;
    let sc4 = shape_card(
        ctx,
        || RectStyle::filled(theme().purple, BorderRadius::all(40.0)),
        "pill radius",
        || theme().on_color,
    )?;
    let cards = Container::new(
        ctx,
        LayoutStyle::new().flex_row().gap(16.0),
        children![sc1, sc2, sc3, sc4],
    )?;
    section(ctx, "Shapes", cards)
}
