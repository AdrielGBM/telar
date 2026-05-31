use crate::theme::{SandboxTheme, heading};
use rsx::{
    BorderRadius, Color, Container, DrawCommand, DrawingArea, LayoutError, LayoutItem, LayoutStyle,
    Paint, Rect, RectPayload, RectStyle, RenderNode, Stroke, TextPayload, TextStyle, WidgetCtx,
    use_theme,
};
use std::rc::Rc;

pub fn shape_card(
    ctx: &mut WidgetCtx,
    style_fn: impl Fn() -> RectStyle + 'static,
    label: &'static str,
    label_color_fn: impl Fn() -> Color + 'static,
) -> Result<DrawingArea, LayoutError> {
    DrawingArea::new(
        ctx,
        LayoutStyle::new().width(168.0).height(80.0),
        move |_| {
            RenderNode::group([
                RenderNode::Primitive(DrawCommand::Rect(Box::new(RectPayload {
                    rect: Rect {
                        x: 0.0,
                        y: 0.0,
                        width: 168.0,
                        height: 80.0,
                    },
                    style: style_fn(),
                }))),
                RenderNode::Primitive(DrawCommand::Text(Box::new(TextPayload {
                    text: Rc::from(label),
                    rect: Rect {
                        x: 0.0,
                        y: 4.0,
                        width: 168.0,
                        height: 72.0,
                    },
                    style: TextStyle::new(13.0, label_color_fn()),
                }))),
            ])
        },
    )
}

pub fn shapes_section(ctx: &mut WidgetCtx) -> Result<Container, LayoutError> {
    let sc1 = Box::new(shape_card(
        ctx,
        || RectStyle {
            fill: Some(Paint::Solid(use_theme::<SandboxTheme>().primary)),
            stroke: None,
            shadow: None,
            radius: BorderRadius::all(8.0),
        },
        "fill",
        || use_theme::<SandboxTheme>().on_color,
    )?) as Box<dyn LayoutItem>;
    let sc2 = Box::new(shape_card(
        ctx,
        || RectStyle {
            fill: None,
            stroke: Some(Stroke::new(use_theme::<SandboxTheme>().danger, 2.0)),
            shadow: None,
            radius: BorderRadius::all(8.0),
        },
        "stroke",
        || use_theme::<SandboxTheme>().danger,
    )?) as Box<dyn LayoutItem>;
    let sc3 = Box::new(shape_card(
        ctx,
        || {
            let t = use_theme::<SandboxTheme>();
            RectStyle {
                fill: Some(Paint::Solid(t.success)),
                stroke: Some(Stroke::new(t.dark, 1.5)),
                shadow: None,
                radius: BorderRadius::zero(),
            }
        },
        "fill + stroke",
        || use_theme::<SandboxTheme>().on_color,
    )?) as Box<dyn LayoutItem>;
    let sc4 = Box::new(shape_card(
        ctx,
        || RectStyle {
            fill: Some(Paint::Solid(use_theme::<SandboxTheme>().purple)),
            stroke: None,
            shadow: None,
            radius: BorderRadius::all(40.0),
        },
        "pill radius",
        || use_theme::<SandboxTheme>().on_color,
    )?) as Box<dyn LayoutItem>;
    let cards = Container::new(
        ctx,
        LayoutStyle::new().flex_row().gap(16.0),
        vec![sc1, sc2, sc3, sc4],
    )?;
    let h = heading(ctx, "Shapes")?;
    Container::new(
        ctx,
        LayoutStyle::new().flex_column().gap(8.0),
        vec![h, Box::new(cards) as Box<dyn LayoutItem>],
    )
}
