use crate::theme::{SandboxTheme, heading};
use rsx::{
    BorderRadius, Color, Container, DrawCommand, DrawingArea, LayoutError, LayoutItem, LayoutStyle,
    Paint, Rect, RectPayload, RectStyle, Stroke, TextPayload, TextStyle, View, WidgetCtx,
    use_theme,
};
use std::rc::Rc;

pub fn info_card(
    ctx: &mut WidgetCtx,
    bg_fn: impl Fn() -> RectStyle + 'static,
    title: &'static str,
    title_color_fn: impl Fn() -> Color + 'static,
    body: &'static str,
) -> Result<DrawingArea, LayoutError> {
    DrawingArea::new(
        ctx,
        LayoutStyle::new().width(368.0).height(110.0),
        move |_w, _h| {
            View::group([
                View::Primitive(DrawCommand::Rect(Box::new(RectPayload {
                    rect: Rect {
                        x: 0.0,
                        y: 0.0,
                        width: 368.0,
                        height: 110.0,
                    },
                    style: bg_fn(),
                }))),
                View::Primitive(DrawCommand::Text(Box::new(TextPayload {
                    text: Rc::from(title),
                    rect: Rect {
                        x: 16.0,
                        y: 14.0,
                        width: 340.0,
                        height: 24.0,
                    },
                    style: TextStyle::new(16.0, title_color_fn()),
                }))),
                View::Primitive(DrawCommand::Text(Box::new(TextPayload {
                    text: Rc::from(body),
                    rect: Rect {
                        x: 16.0,
                        y: 44.0,
                        width: 340.0,
                        height: 52.0,
                    },
                    style: TextStyle::new(13.0, use_theme::<SandboxTheme>().muted),
                }))),
            ])
        },
    )
}

pub fn cards_section(ctx: &mut WidgetCtx) -> Result<Container, LayoutError> {
    let c1 = Box::new(info_card(
        ctx,
        || RectStyle {
            fill: Some(Paint::Solid(use_theme::<SandboxTheme>().dark)),
            stroke: None,
            shadow: None,
            radius: BorderRadius::all(10.0),
        },
        "Dark Card",
        || Color::WHITE,
        "White text on a dark background.",
    )?) as Box<dyn LayoutItem>;
    let c2 = Box::new(info_card(
        ctx,
        || RectStyle {
            fill: Some(Paint::Solid(Color::WHITE)),
            stroke: Some(Stroke::new(use_theme::<SandboxTheme>().card_border, 1.0)),
            shadow: None,
            radius: BorderRadius::all(10.0),
        },
        "Light Card",
        || use_theme::<SandboxTheme>().dark,
        "Dark text on a white background.",
    )?) as Box<dyn LayoutItem>;
    let row = Container::new(ctx, LayoutStyle::new().flex_row().gap(16.0), vec![c1, c2])?;
    let h = heading(ctx, "Cards")?;
    Container::new(
        ctx,
        LayoutStyle::new().flex_column().gap(8.0),
        vec![h, Box::new(row) as Box<dyn LayoutItem>],
    )
}
