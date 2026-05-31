use std::rc::Rc;

use rsx::{
    BorderRadius, Color, Component, DrawCommand, DrawingArea, FillRule, Gradient, LayoutError,
    LayoutStyle, Line, LineStyle, Paint, Path, PathData, PathStyle, Point, Rect, RectPayload,
    RectStyle, RenderNode, Stroke, TextPayload, TextStyle, WidgetCtx, use_theme,
};

use crate::theme::SandboxTheme;

pub fn gradients_section(ctx: &mut WidgetCtx) -> Result<DrawingArea, LayoutError> {
    DrawingArea::new(ctx, LayoutStyle::new().height(520.0), |w, _h| {
        let t = use_theme::<SandboxTheme>();
        let primary = t.primary;
        let success = t.success;
        let danger = t.danger;
        let warning = t.warning;
        let purple = t.purple;
        let dark = t.dark;
        let muted = t.muted;
        let card_border = t.card_border;
        let cyan = t.cyan;

        let mut children: Vec<RenderNode> = Vec::new();

        children.push(
            Line::new(
                || Point::new(0.0, 0.0),
                move || Point::new(w, 0.0),
                move || LineStyle::new(card_border, 1.0),
            )
            .view(),
        );
        children.push(RenderNode::Primitive(DrawCommand::Text(Box::new(
            TextPayload {
                text: Rc::from("Gradients"),
                rect: Rect {
                    x: 0.0,
                    y: 12.0,
                    width: 200.0,
                    height: 20.0,
                },
                style: TextStyle::new(12.0, muted),
            },
        ))));

        children.push(RenderNode::Primitive(DrawCommand::Text(Box::new(
            TextPayload {
                text: Rc::from("Linear — Rect"),
                rect: Rect {
                    x: 0.0,
                    y: 40.0,
                    width: 300.0,
                    height: 16.0,
                },
                style: TextStyle::new(11.0, muted),
            },
        ))));

        children.push(RenderNode::Primitive(DrawCommand::Rect(Box::new(
            RectPayload {
                rect: Rect {
                    x: 0.0,
                    y: 60.0,
                    width: 168.0,
                    height: 80.0,
                },
                style: RectStyle {
                    fill: Some(Paint::Gradient(Gradient::linear(
                        Point::new(0.0, 100.0),
                        Point::new(168.0, 100.0),
                        &[(0.0, danger), (1.0, primary)],
                    ))),
                    stroke: None,
                    shadow: None,
                    radius: BorderRadius::all(8.0),
                },
            },
        ))));
        children.push(RenderNode::Primitive(DrawCommand::Text(Box::new(
            TextPayload {
                text: Rc::from("horizontal"),
                rect: Rect {
                    x: 0.0,
                    y: 146.0,
                    width: 168.0,
                    height: 16.0,
                },
                style: TextStyle::new(11.0, muted),
            },
        ))));

        children.push(RenderNode::Primitive(DrawCommand::Rect(Box::new(
            RectPayload {
                rect: Rect {
                    x: 184.0,
                    y: 60.0,
                    width: 168.0,
                    height: 80.0,
                },
                style: RectStyle {
                    fill: Some(Paint::Gradient(Gradient::linear(
                        Point::new(268.0, 60.0),
                        Point::new(268.0, 140.0),
                        &[(0.0, purple), (1.0, success)],
                    ))),
                    stroke: None,
                    shadow: None,
                    radius: BorderRadius::all(8.0),
                },
            },
        ))));
        children.push(RenderNode::Primitive(DrawCommand::Text(Box::new(
            TextPayload {
                text: Rc::from("vertical"),
                rect: Rect {
                    x: 184.0,
                    y: 146.0,
                    width: 168.0,
                    height: 16.0,
                },
                style: TextStyle::new(11.0, muted),
            },
        ))));

        children.push(RenderNode::Primitive(DrawCommand::Rect(Box::new(
            RectPayload {
                rect: Rect {
                    x: 368.0,
                    y: 60.0,
                    width: 168.0,
                    height: 80.0,
                },
                style: RectStyle {
                    fill: Some(Paint::Gradient(Gradient::linear(
                        Point::new(368.0, 60.0),
                        Point::new(536.0, 140.0),
                        &[(0.0, warning), (1.0, dark)],
                    ))),
                    stroke: None,
                    shadow: None,
                    radius: BorderRadius::all(8.0),
                },
            },
        ))));
        children.push(RenderNode::Primitive(DrawCommand::Text(Box::new(
            TextPayload {
                text: Rc::from("diagonal"),
                rect: Rect {
                    x: 368.0,
                    y: 146.0,
                    width: 168.0,
                    height: 16.0,
                },
                style: TextStyle::new(11.0, muted),
            },
        ))));

        children.push(RenderNode::Primitive(DrawCommand::Rect(Box::new(
            RectPayload {
                rect: Rect {
                    x: 552.0,
                    y: 60.0,
                    width: 168.0,
                    height: 80.0,
                },
                style: RectStyle {
                    fill: Some(Paint::Gradient(Gradient::linear(
                        Point::new(552.0, 100.0),
                        Point::new(720.0, 100.0),
                        &[(0.0, dark), (0.5, cyan), (1.0, Color::WHITE)],
                    ))),
                    stroke: None,
                    shadow: None,
                    radius: BorderRadius::all(8.0),
                },
            },
        ))));
        children.push(RenderNode::Primitive(DrawCommand::Text(Box::new(
            TextPayload {
                text: Rc::from("3 stops"),
                rect: Rect {
                    x: 552.0,
                    y: 146.0,
                    width: 168.0,
                    height: 16.0,
                },
                style: TextStyle::new(11.0, muted),
            },
        ))));

        children.push(RenderNode::Primitive(DrawCommand::Text(Box::new(
            TextPayload {
                text: Rc::from("Radial — Rect"),
                rect: Rect {
                    x: 0.0,
                    y: 180.0,
                    width: 300.0,
                    height: 16.0,
                },
                style: TextStyle::new(11.0, muted),
            },
        ))));

        children.push(RenderNode::Primitive(DrawCommand::Rect(Box::new(
            RectPayload {
                rect: Rect {
                    x: 0.0,
                    y: 200.0,
                    width: 168.0,
                    height: 80.0,
                },
                style: RectStyle {
                    fill: Some(Paint::Gradient(Gradient::radial(
                        Point::new(84.0, 240.0),
                        70.0,
                        &[(0.0, primary), (1.0, primary.with_alpha(0.0))],
                    ))),
                    stroke: None,
                    shadow: None,
                    radius: BorderRadius::all(8.0),
                },
            },
        ))));
        children.push(RenderNode::Primitive(DrawCommand::Text(Box::new(
            TextPayload {
                text: Rc::from("center burst"),
                rect: Rect {
                    x: 0.0,
                    y: 286.0,
                    width: 168.0,
                    height: 16.0,
                },
                style: TextStyle::new(11.0, muted),
            },
        ))));

        children.push(RenderNode::Primitive(DrawCommand::Rect(Box::new(
            RectPayload {
                rect: Rect {
                    x: 184.0,
                    y: 200.0,
                    width: 168.0,
                    height: 80.0,
                },
                style: RectStyle {
                    fill: Some(Paint::Gradient(Gradient::radial(
                        Point::new(268.0, 240.0),
                        40.0,
                        &[(0.0, danger), (1.0, warning)],
                    ))),
                    stroke: None,
                    shadow: None,
                    radius: BorderRadius::all(8.0),
                },
            },
        ))));
        children.push(RenderNode::Primitive(DrawCommand::Text(Box::new(
            TextPayload {
                text: Rc::from("tight radius"),
                rect: Rect {
                    x: 184.0,
                    y: 286.0,
                    width: 168.0,
                    height: 16.0,
                },
                style: TextStyle::new(11.0, muted),
            },
        ))));

        children.push(RenderNode::Primitive(DrawCommand::Rect(Box::new(
            RectPayload {
                rect: Rect {
                    x: 368.0,
                    y: 200.0,
                    width: 168.0,
                    height: 80.0,
                },
                style: RectStyle {
                    fill: Some(Paint::Gradient(Gradient::radial(
                        Point::new(452.0, 240.0),
                        80.0,
                        &[(0.0, Color::WHITE), (0.45, purple), (1.0, dark)],
                    ))),
                    stroke: None,
                    shadow: None,
                    radius: BorderRadius::all(8.0),
                },
            },
        ))));
        children.push(RenderNode::Primitive(DrawCommand::Text(Box::new(
            TextPayload {
                text: Rc::from("3 stops"),
                rect: Rect {
                    x: 368.0,
                    y: 286.0,
                    width: 168.0,
                    height: 16.0,
                },
                style: TextStyle::new(11.0, muted),
            },
        ))));

        children.push(RenderNode::Primitive(DrawCommand::Rect(Box::new(
            RectPayload {
                rect: Rect {
                    x: 552.0,
                    y: 200.0,
                    width: 168.0,
                    height: 80.0,
                },
                style: RectStyle {
                    fill: Some(Paint::Gradient(Gradient::radial(
                        Point::new(552.0, 200.0),
                        180.0,
                        &[(0.0, success), (1.0, dark)],
                    ))),
                    stroke: None,
                    shadow: None,
                    radius: BorderRadius::all(8.0),
                },
            },
        ))));
        children.push(RenderNode::Primitive(DrawCommand::Text(Box::new(
            TextPayload {
                text: Rc::from("off-center"),
                rect: Rect {
                    x: 552.0,
                    y: 286.0,
                    width: 168.0,
                    height: 16.0,
                },
                style: TextStyle::new(11.0, muted),
            },
        ))));

        children.push(RenderNode::Primitive(DrawCommand::Text(Box::new(
            TextPayload {
                text: Rc::from("Gradients — Path"),
                rect: Rect {
                    x: 0.0,
                    y: 318.0,
                    width: 300.0,
                    height: 16.0,
                },
                style: TextStyle::new(11.0, muted),
            },
        ))));

        let tri = Rc::new(
            PathData::new()
                .move_to(Point::new(75.0, 338.0))
                .line_to(Point::new(150.0, 468.0))
                .line_to(Point::new(0.0, 468.0))
                .close(),
        );
        children.push(
            Path::new(
                {
                    let d = tri.clone();
                    move || d.clone()
                },
                move || PathStyle {
                    fill: Some(Paint::Gradient(Gradient::linear(
                        Point::new(75.0, 338.0),
                        Point::new(75.0, 468.0),
                        &[(0.0, danger), (1.0, warning)],
                    ))),
                    stroke: None,
                    shadow: None,
                    fill_rule: FillRule::Winding,
                },
            )
            .view(),
        );
        children.push(RenderNode::Primitive(DrawCommand::Text(Box::new(
            TextPayload {
                text: Rc::from("triangle linear"),
                rect: Rect {
                    x: 0.0,
                    y: 476.0,
                    width: 180.0,
                    height: 16.0,
                },
                style: TextStyle::new(11.0, muted),
            },
        ))));

        let cx = 268.0f32;
        let cy = 403.0f32;
        let outer = 65.0f32;
        let inner = 26.0f32;
        let mut star_path = PathData::new();
        for i in 0..10usize {
            let angle = std::f32::consts::TAU * i as f32 / 10.0 - std::f32::consts::FRAC_PI_2;
            let r = if i % 2 == 0 { outer } else { inner };
            let p = Point::new(cx + r * angle.cos(), cy + r * angle.sin());
            star_path = if i == 0 {
                star_path.move_to(p)
            } else {
                star_path.line_to(p)
            };
        }
        let star_path = Rc::new(star_path.close());
        children.push(
            Path::new(
                {
                    let d = star_path.clone();
                    move || d.clone()
                },
                move || PathStyle {
                    fill: Some(Paint::Gradient(Gradient::radial(
                        Point::new(268.0, 403.0),
                        65.0,
                        &[(0.0, Color::WHITE), (0.5, purple), (1.0, dark)],
                    ))),
                    stroke: Some(Stroke::new(dark, 1.0)),
                    shadow: None,
                    fill_rule: FillRule::Winding,
                },
            )
            .view(),
        );
        children.push(RenderNode::Primitive(DrawCommand::Text(Box::new(
            TextPayload {
                text: Rc::from("star radial"),
                rect: Rect {
                    x: 200.0,
                    y: 476.0,
                    width: 180.0,
                    height: 16.0,
                },
                style: TextStyle::new(11.0, muted),
            },
        ))));

        let petal = Rc::new(
            PathData::new()
                .move_to(Point::new(452.0, 338.0))
                .cubic_to(
                    Point::new(532.0, 338.0),
                    Point::new(532.0, 468.0),
                    Point::new(452.0, 468.0),
                )
                .cubic_to(
                    Point::new(372.0, 468.0),
                    Point::new(372.0, 338.0),
                    Point::new(452.0, 338.0),
                )
                .close(),
        );
        children.push(
            Path::new(
                {
                    let d = petal.clone();
                    move || d.clone()
                },
                move || PathStyle {
                    fill: Some(Paint::Gradient(Gradient::linear(
                        Point::new(372.0, 338.0),
                        Point::new(532.0, 468.0),
                        &[(0.0, success), (0.5, cyan), (1.0, primary)],
                    ))),
                    stroke: Some(Stroke::new(dark, 1.5)),
                    shadow: None,
                    fill_rule: FillRule::Winding,
                },
            )
            .view(),
        );
        children.push(RenderNode::Primitive(DrawCommand::Text(Box::new(
            TextPayload {
                text: Rc::from("petal linear 3-stop"),
                rect: Rect {
                    x: 372.0,
                    y: 476.0,
                    width: 180.0,
                    height: 16.0,
                },
                style: TextStyle::new(11.0, muted),
            },
        ))));

        let rings = Rc::new(
            PathData::new()
                .move_to(Point::new(576.0, 338.0))
                .line_to(Point::new(736.0, 338.0))
                .line_to(Point::new(736.0, 468.0))
                .line_to(Point::new(576.0, 468.0))
                .close()
                .move_to(Point::new(600.0, 362.0))
                .line_to(Point::new(712.0, 362.0))
                .line_to(Point::new(712.0, 444.0))
                .line_to(Point::new(600.0, 444.0))
                .close(),
        );
        children.push(
            Path::new(
                {
                    let d = rings.clone();
                    move || d.clone()
                },
                move || PathStyle {
                    fill: Some(Paint::Gradient(Gradient::linear(
                        Point::new(576.0, 403.0),
                        Point::new(736.0, 403.0),
                        &[(0.0, danger), (1.0, purple)],
                    ))),
                    stroke: None,
                    shadow: None,
                    fill_rule: FillRule::EvenOdd,
                },
            )
            .view(),
        );
        children.push(RenderNode::Primitive(DrawCommand::Text(Box::new(
            TextPayload {
                text: Rc::from("even-odd + linear"),
                rect: Rect {
                    x: 576.0,
                    y: 476.0,
                    width: 180.0,
                    height: 16.0,
                },
                style: TextStyle::new(11.0, muted),
            },
        ))));

        RenderNode::group(children)
    })
}
