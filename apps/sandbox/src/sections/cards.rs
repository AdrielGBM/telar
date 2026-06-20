use crate::theme::{section, theme};
use rsx::{
    BorderRadius, Canvas, Color, Container, LayoutError, LayoutStyle, Rect, RectStyle, RenderNode,
    ShapeStyle, Stroke, TextStyle, WidgetCtx, children,
};

pub fn info_card(
    ctx: &mut WidgetCtx,
    bg_fn: impl Fn() -> RectStyle + 'static,
    title: &'static str,
    title_color_fn: impl Fn() -> Color + 'static,
    body: &'static str,
) -> Result<Canvas, LayoutError> {
    Canvas::new(
        ctx,
        LayoutStyle::new().width(368.0).height(110.0),
        move |_| {
            RenderNode::group([
                RenderNode::rect(
                    Rect {
                        x: 0.0,
                        y: 0.0,
                        width: 368.0,
                        height: 110.0,
                    },
                    bg_fn(),
                ),
                RenderNode::text(
                    title,
                    Rect {
                        x: 16.0,
                        y: 14.0,
                        width: 340.0,
                        height: 24.0,
                    },
                    TextStyle::new(16.0, title_color_fn()),
                ),
                RenderNode::text(
                    body,
                    Rect {
                        x: 16.0,
                        y: 44.0,
                        width: 340.0,
                        height: 52.0,
                    },
                    TextStyle::new(13.0, theme().muted),
                ),
            ])
        },
    )
}

pub fn cards_section(ctx: &mut WidgetCtx) -> Result<Container, LayoutError> {
    let c1 = info_card(
        ctx,
        || RectStyle::filled(theme().dark, BorderRadius::all(10.0)),
        "Dark Card",
        || Color::WHITE,
        "White text on a dark background.",
    )?;
    let c2 = info_card(
        ctx,
        || {
            RectStyle::filled(Color::WHITE, BorderRadius::all(10.0))
                .with_stroke(Stroke::new(theme().card_border, 1.0))
        },
        "Light Card",
        || theme().dark,
        "Dark text on a white background.",
    )?;
    let row = Container::new(
        ctx,
        LayoutStyle::new().flex_row().gap(16.0),
        children![c1, c2],
    )?;
    section(ctx, "Cards", row)
}
