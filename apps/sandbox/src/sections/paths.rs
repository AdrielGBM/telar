use std::rc::Rc;

use rsx::{
    Bounds, Color, Component, DrawCommand, DrawingArea, FillRule, FillStyle, LayoutError,
    LayoutStyle, Line, LineCap, LineJoin, LineStyle, Path, PathData, PathStyle, Point, Shadow,
    Stroke, TextPayload, TextStyle, View, WidgetCtx, use_theme,
};

use crate::theme::SandboxTheme;

pub fn paths_section(ctx: &mut WidgetCtx) -> Result<DrawingArea, LayoutError> {
    DrawingArea::new(ctx, LayoutStyle::new().height(660.0), |w, _h| {
        let t = use_theme::<SandboxTheme>();
        let primary = t.primary;
        let success = t.success;
        let danger = t.danger;
        let warning = t.warning;
        let purple = t.purple;
        let dark = t.dark;
        let muted = t.muted;
        let card_border = t.card_border;

        let mut children: Vec<View> = Vec::new();

        children.push(
            Line::new(
                || Point::new(0.0, 0.0),
                move || Point::new(w, 0.0),
                move || LineStyle::new(card_border, 1.0),
            )
            .view(),
        );
        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("Paths"),
            rect: Bounds {
                x: 0.0,
                y: 12.0,
                width: 200.0,
                height: 20.0,
            },
            style: TextStyle::new(12.0, muted),
        }))));
        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("Polygon shapes"),
            rect: Bounds {
                x: 0.0,
                y: 36.0,
                width: 300.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, muted),
        }))));

        let triangle_data = Rc::new(
            PathData::new()
                .move_to(Point::new(75.0, 56.0))
                .line_to(Point::new(135.0, 166.0))
                .line_to(Point::new(15.0, 166.0))
                .close(),
        );
        children.push(
            Path::new(
                {
                    let d = triangle_data.clone();
                    move || d.clone()
                },
                move || PathStyle {
                    fill: Some(FillStyle::Solid(primary)),
                    stroke: None,
                    fill_rule: FillRule::Winding,
                    shadow: None,
                },
            )
            .view(),
        );
        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("triangle"),
            rect: Bounds {
                x: 0.0,
                y: 176.0,
                width: 150.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, muted),
        }))));

        {
            let cx = 245.0f32;
            let cy = 111.0f32;
            let outer = 55.0f32;
            let inner = 22.0f32;
            let mut path = PathData::new();
            for i in 0..10usize {
                let angle = std::f32::consts::TAU * i as f32 / 10.0 - std::f32::consts::FRAC_PI_2;
                let r = if i % 2 == 0 { outer } else { inner };
                let p = Point::new(cx + r * angle.cos(), cy + r * angle.sin());
                path = if i == 0 {
                    path.move_to(p)
                } else {
                    path.line_to(p)
                };
            }
            path = path.close();
            let star_data = Rc::new(path);
            children.push(
                Path::new(
                    {
                        let d = star_data.clone();
                        move || d.clone()
                    },
                    move || PathStyle {
                        fill: Some(FillStyle::Solid(danger)),
                        stroke: Some(Stroke::new(dark, 1.0)),
                        fill_rule: FillRule::Winding,
                        shadow: None,
                    },
                )
                .view(),
            );
            children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
                text: Rc::from("star (fill + stroke)"),
                rect: Bounds {
                    x: 175.0,
                    y: 176.0,
                    width: 200.0,
                    height: 16.0,
                },
                style: TextStyle::new(11.0, muted),
            }))));
        }

        let evenodd_data = Rc::new(
            PathData::new()
                .move_to(Point::new(360.0, 58.0))
                .line_to(Point::new(540.0, 58.0))
                .line_to(Point::new(540.0, 168.0))
                .line_to(Point::new(360.0, 168.0))
                .close()
                .move_to(Point::new(390.0, 88.0))
                .line_to(Point::new(510.0, 88.0))
                .line_to(Point::new(510.0, 138.0))
                .line_to(Point::new(390.0, 138.0))
                .close(),
        );
        children.push(
            Path::new(
                {
                    let d = evenodd_data.clone();
                    move || d.clone()
                },
                move || PathStyle {
                    fill: Some(FillStyle::Solid(purple)),
                    stroke: None,
                    fill_rule: FillRule::EvenOdd,
                    shadow: None,
                },
            )
            .view(),
        );
        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("even-odd fill"),
            rect: Bounds {
                x: 350.0,
                y: 176.0,
                width: 200.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, muted),
        }))));

        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("Bézier curves"),
            rect: Bounds {
                x: 0.0,
                y: 212.0,
                width: 300.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, muted),
        }))));

        let quad_data = Rc::new(
            PathData::new()
                .move_to(Point::new(0.0, 308.0))
                .quad_to(Point::new(140.0, 238.0), Point::new(280.0, 308.0)),
        );
        children.push(
            Path::new(
                {
                    let d = quad_data.clone();
                    move || d.clone()
                },
                move || PathStyle {
                    fill: None,
                    stroke: Some(Stroke::new(warning, 3.0).with_cap(LineCap::Round)),
                    fill_rule: FillRule::Winding,
                    shadow: None,
                },
            )
            .view(),
        );
        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("quad_to arch"),
            rect: Bounds {
                x: 0.0,
                y: 318.0,
                width: 200.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, muted),
        }))));

        let cubic_data = Rc::new(PathData::new().move_to(Point::new(310.0, 248.0)).cubic_to(
            Point::new(380.0, 248.0),
            Point::new(310.0, 308.0),
            Point::new(380.0, 308.0),
        ));
        children.push(
            Path::new(
                {
                    let d = cubic_data.clone();
                    move || d.clone()
                },
                move || PathStyle {
                    fill: None,
                    stroke: Some(Stroke::new(success, 3.0).with_cap(LineCap::Round)),
                    fill_rule: FillRule::Winding,
                    shadow: None,
                },
            )
            .view(),
        );
        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("cubic_to S-curve"),
            rect: Bounds {
                x: 296.0,
                y: 318.0,
                width: 200.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, muted),
        }))));

        let petal_data = Rc::new(
            PathData::new()
                .move_to(Point::new(516.0, 243.0))
                .cubic_to(
                    Point::new(586.0, 243.0),
                    Point::new(586.0, 313.0),
                    Point::new(516.0, 313.0),
                )
                .cubic_to(
                    Point::new(446.0, 313.0),
                    Point::new(446.0, 243.0),
                    Point::new(516.0, 243.0),
                )
                .close(),
        );
        children.push(
            Path::new(
                {
                    let d = petal_data.clone();
                    move || d.clone()
                },
                move || PathStyle {
                    fill: Some(FillStyle::Solid(Color { a: 0.75, ..warning })),
                    stroke: Some(Stroke::new(warning, 1.5)),
                    fill_rule: FillRule::Winding,
                    shadow: None,
                },
            )
            .view(),
        );
        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("closed cubic (petal)"),
            rect: Bounds {
                x: 446.0,
                y: 318.0,
                width: 200.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, muted),
        }))));

        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("Stroke style"),
            rect: Bounds {
                x: 0.0,
                y: 354.0,
                width: 300.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, muted),
        }))));

        let butt_data = Rc::new(
            PathData::new()
                .move_to(Point::new(0.0, 410.0))
                .line_to(Point::new(76.0, 390.0))
                .line_to(Point::new(152.0, 430.0))
                .line_to(Point::new(228.0, 390.0)),
        );
        children.push(
            Path::new(
                {
                    let d = butt_data.clone();
                    move || d.clone()
                },
                move || PathStyle {
                    fill: None,
                    stroke: Some(Stroke::new(primary, 8.0)),
                    fill_rule: FillRule::Winding,
                    shadow: None,
                },
            )
            .view(),
        );
        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("Butt / Miter (default)"),
            rect: Bounds {
                x: 0.0,
                y: 448.0,
                width: 230.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, muted),
        }))));

        let round_data = Rc::new(
            PathData::new()
                .move_to(Point::new(300.0, 410.0))
                .line_to(Point::new(376.0, 390.0))
                .line_to(Point::new(452.0, 430.0))
                .line_to(Point::new(528.0, 390.0)),
        );
        children.push(
            Path::new(
                {
                    let d = round_data.clone();
                    move || d.clone()
                },
                move || PathStyle {
                    fill: None,
                    stroke: Some(
                        Stroke::new(danger, 8.0)
                            .with_cap(LineCap::Round)
                            .with_join(LineJoin::Round),
                    ),
                    fill_rule: FillRule::Winding,
                    shadow: None,
                },
            )
            .view(),
        );
        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("Round cap / Round join"),
            rect: Bounds {
                x: 300.0,
                y: 448.0,
                width: 240.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, muted),
        }))));

        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("Path shadows"),
            rect: Bounds {
                x: 0.0,
                y: 490.0,
                width: 300.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, muted),
        }))));

        const K: f32 = 0.5523;
        let (cx1, cy1, r1) = (76.0_f32, 570.0_f32, 44.0_f32);
        let circle_data = Rc::new(
            PathData::new()
                .move_to(Point::new(cx1, cy1 - r1))
                .cubic_to(
                    Point::new(cx1 + K * r1, cy1 - r1),
                    Point::new(cx1 + r1, cy1 - K * r1),
                    Point::new(cx1 + r1, cy1),
                )
                .cubic_to(
                    Point::new(cx1 + r1, cy1 + K * r1),
                    Point::new(cx1 + K * r1, cy1 + r1),
                    Point::new(cx1, cy1 + r1),
                )
                .cubic_to(
                    Point::new(cx1 - K * r1, cy1 + r1),
                    Point::new(cx1 - r1, cy1 + K * r1),
                    Point::new(cx1 - r1, cy1),
                )
                .cubic_to(
                    Point::new(cx1 - r1, cy1 - K * r1),
                    Point::new(cx1 - K * r1, cy1 - r1),
                    Point::new(cx1, cy1 - r1),
                )
                .close(),
        );
        children.push(
            Path::new(
                {
                    let d = circle_data.clone();
                    move || d.clone()
                },
                move || {
                    PathStyle::default()
                        .with_fill(primary)
                        .with_shadow(Shadow::new(4.0, 6.0, 8.0, Color::rgba(0.0, 0.0, 0.0, 0.4)))
                },
            )
            .view(),
        );
        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("drop shadow"),
            rect: Bounds {
                x: 32.0,
                y: 624.0,
                width: 88.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, muted),
        }))));

        let star_shadow_data = Rc::new({
            let cx = 272.0_f32;
            let cy = 570.0_f32;
            let outer = 44.0_f32;
            let inner = 18.0_f32;
            let n = 5usize;
            let mut pd = PathData::new();
            for i in 0..n * 2 {
                let r = if i % 2 == 0 { outer } else { inner };
                let angle =
                    std::f32::consts::PI * i as f32 / n as f32 - std::f32::consts::FRAC_PI_2;
                let p = Point::new(cx + r * angle.cos(), cy + r * angle.sin());
                if i == 0 {
                    pd = pd.move_to(p);
                } else {
                    pd = pd.line_to(p);
                }
            }
            pd.close()
        });
        children.push(
            Path::new(
                {
                    let d = star_shadow_data.clone();
                    move || d.clone()
                },
                move || {
                    PathStyle::default()
                        .with_fill(warning)
                        .with_shadow(Shadow::new(0.0, 0.0, 10.0, Color { a: 0.7, ..warning }))
                },
            )
            .view(),
        );
        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("glow"),
            rect: Bounds {
                x: 248.0,
                y: 624.0,
                width: 48.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, muted),
        }))));

        let (cx3, cy3, r3) = (468.0_f32, 570.0_f32, 44.0_f32);
        let diamond_data = Rc::new(
            PathData::new()
                .move_to(Point::new(cx3, cy3 - r3))
                .line_to(Point::new(cx3 + r3 * 0.65, cy3))
                .line_to(Point::new(cx3, cy3 + r3))
                .line_to(Point::new(cx3 - r3 * 0.65, cy3))
                .close(),
        );
        children.push(
            Path::new(
                {
                    let d = diamond_data.clone();
                    move || d.clone()
                },
                move || {
                    PathStyle::default()
                        .with_fill(success)
                        .with_shadow(Shadow::new(3.0, 3.0, 2.0, Color::rgba(0.0, 0.0, 0.0, 0.5)))
                },
            )
            .view(),
        );
        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("hard offset"),
            rect: Bounds {
                x: 428.0,
                y: 624.0,
                width: 80.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, muted),
        }))));

        let wave_data = Rc::new(
            PathData::new()
                .move_to(Point::new(556.0, 570.0))
                .cubic_to(
                    Point::new(586.0, 530.0),
                    Point::new(626.0, 530.0),
                    Point::new(656.0, 570.0),
                )
                .cubic_to(
                    Point::new(686.0, 610.0),
                    Point::new(726.0, 610.0),
                    Point::new(756.0, 570.0),
                ),
        );
        children.push(
            Path::new(
                {
                    let d = wave_data.clone();
                    move || d.clone()
                },
                move || {
                    PathStyle::default()
                        .with_stroke(Stroke::new(danger, 4.0).with_cap(LineCap::Round))
                        .with_shadow(Shadow::new(2.0, 4.0, 6.0, Color { a: 0.5, ..danger }))
                },
            )
            .view(),
        );
        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("stroke shadow"),
            rect: Bounds {
                x: 600.0,
                y: 624.0,
                width: 100.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, muted),
        }))));

        View::group(children)
    })
}
