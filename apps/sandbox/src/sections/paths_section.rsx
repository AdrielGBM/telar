[logic]
use std::sync::Arc;
use crate::theme::theme;

let result = Canvas::with_intrinsic_height(ctx, 620.0, |rect| {
    let t = theme();
    let primary = t.primary;
    let success = t.success;
    let danger = t.danger;
    let warning = t.warning;
    let purple = t.purple;
    let dark = t.dark;
    let muted = t.muted;

    let mut children: Vec<RenderNode> = Vec::new();

    children.push(RenderNode::text(
        rsx::static_rc_str!("Polygon shapes"),
        Rect { x: 0.0, y: 36.0, width: 300.0, height: 16.0 },
        TextStyle::new(11.0, muted),
    ));

    let triangle_data = Arc::new(PathData::polygon(&[
        Point::new(75.0, 56.0),
        Point::new(135.0, 166.0),
        Point::new(15.0, 166.0),
    ]));
    children.push(
        Path::static_data(triangle_data.clone(), move || PathStyle {
            fill: Some(Paint::Solid(primary)),
            stroke: None,
            shadow: None,
            fill_rule: FillRule::Winding,
        })
        .view(),
    );
    children.push(RenderNode::text(
        rsx::static_rc_str!("triangle"),
        Rect { x: 0.0, y: 176.0, width: 150.0, height: 16.0 },
        TextStyle::new(11.0, muted),
    ));

    {
        let cx = 245.0f32;
        let cy = 111.0f32;
        let outer = 55.0f32;
        let inner = 22.0f32;
        let points: Vec<Point> = (0..10usize)
            .map(|i| {
                let angle = std::f32::consts::TAU * i as f32 / 10.0 - std::f32::consts::FRAC_PI_2;
                let r = if i % 2 == 0 { outer } else { inner };
                Point::new(cx + r * angle.cos(), cy + r * angle.sin())
            })
            .collect();
        let star_data = Arc::new(PathData::polygon(&points));
        children.push(
            Path::static_data(star_data.clone(), move || PathStyle {
                fill: Some(Paint::Solid(danger)),
                stroke: Some(Stroke::new(dark, 1.0)),
                shadow: None,
                fill_rule: FillRule::Winding,
            })
            .view(),
        );
        children.push(RenderNode::text(
            rsx::static_rc_str!("star (fill + stroke)"),
            Rect { x: 175.0, y: 176.0, width: 200.0, height: 16.0 },
            TextStyle::new(11.0, muted),
        ));
    }

    let evenodd_data = Arc::new(
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
        Path::static_data(evenodd_data.clone(), move || PathStyle {
            fill: Some(Paint::Solid(purple)),
            stroke: None,
            shadow: None,
            fill_rule: FillRule::EvenOdd,
        })
        .view(),
    );
    children.push(RenderNode::text(
        rsx::static_rc_str!("even-odd fill"),
        Rect { x: 350.0, y: 176.0, width: 200.0, height: 16.0 },
        TextStyle::new(11.0, muted),
    ));

    children.push(RenderNode::text(
        rsx::static_rc_str!("Bézier curves"),
        Rect { x: 0.0, y: 212.0, width: 300.0, height: 16.0 },
        TextStyle::new(11.0, muted),
    ));

    let quad_data = Arc::new(
        PathData::new()
            .move_to(Point::new(0.0, 308.0))
            .quad_to(Point::new(140.0, 238.0), Point::new(280.0, 308.0)),
    );
    children.push(
        Path::static_data(quad_data.clone(), move || PathStyle {
            fill: None,
            stroke: Some(Stroke::new(warning, 3.0).with_cap(LineCap::Round)),
            shadow: None,
            fill_rule: FillRule::Winding,
        })
        .view(),
    );
    children.push(RenderNode::text(
        rsx::static_rc_str!("quad_to arch"),
        Rect { x: 0.0, y: 318.0, width: 200.0, height: 16.0 },
        TextStyle::new(11.0, muted),
    ));

    let cubic_data = Arc::new(PathData::new().move_to(Point::new(310.0, 248.0)).cubic_to(
        Point::new(380.0, 248.0),
        Point::new(310.0, 308.0),
        Point::new(380.0, 308.0),
    ));
    children.push(
        Path::static_data(cubic_data.clone(), move || PathStyle {
            fill: None,
            stroke: Some(Stroke::new(success, 3.0).with_cap(LineCap::Round)),
            shadow: None,
            fill_rule: FillRule::Winding,
        })
        .view(),
    );
    children.push(RenderNode::text(
        rsx::static_rc_str!("cubic_to S-curve"),
        Rect { x: 296.0, y: 318.0, width: 200.0, height: 16.0 },
        TextStyle::new(11.0, muted),
    ));

    let petal_data = Arc::new(
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
        Path::static_data(petal_data.clone(), move || PathStyle {
            fill: Some(Paint::Solid(warning.with_alpha(0.75))),
            stroke: Some(Stroke::new(warning, 1.5)),
            shadow: None,
            fill_rule: FillRule::Winding,
        })
        .view(),
    );
    children.push(RenderNode::text(
        rsx::static_rc_str!("closed cubic (petal)"),
        Rect { x: 446.0, y: 318.0, width: 200.0, height: 16.0 },
        TextStyle::new(11.0, muted),
    ));

    children.push(RenderNode::text(
        rsx::static_rc_str!("Stroke style"),
        Rect { x: 0.0, y: 354.0, width: 300.0, height: 16.0 },
        TextStyle::new(11.0, muted),
    ));

    let butt_data = Arc::new(
        PathData::new()
            .move_to(Point::new(0.0, 410.0))
            .line_to(Point::new(76.0, 390.0))
            .line_to(Point::new(152.0, 430.0))
            .line_to(Point::new(228.0, 390.0)),
    );
    children.push(
        Path::static_data(butt_data.clone(), move || PathStyle {
            fill: None,
            stroke: Some(Stroke::new(primary, 8.0)),
            shadow: None,
            fill_rule: FillRule::Winding,
        })
        .view(),
    );
    children.push(RenderNode::text(
        rsx::static_rc_str!("Butt / Miter (default)"),
        Rect { x: 0.0, y: 448.0, width: 230.0, height: 16.0 },
        TextStyle::new(11.0, muted),
    ));

    let round_data = Arc::new(
        PathData::new()
            .move_to(Point::new(300.0, 410.0))
            .line_to(Point::new(376.0, 390.0))
            .line_to(Point::new(452.0, 430.0))
            .line_to(Point::new(528.0, 390.0)),
    );
    children.push(
        Path::static_data(round_data.clone(), move || PathStyle {
            fill: None,
            stroke: Some(
                Stroke::new(danger, 8.0)
                    .with_cap(LineCap::Round)
                    .with_join(LineJoin::Round),
            ),
            shadow: None,
            fill_rule: FillRule::Winding,
        })
        .view(),
    );
    children.push(RenderNode::text(
        rsx::static_rc_str!("Round cap / Round join"),
        Rect { x: 300.0, y: 448.0, width: 240.0, height: 16.0 },
        TextStyle::new(11.0, muted),
    ));

    children.push(RenderNode::text(
        rsx::static_rc_str!("Path shadows"),
        Rect { x: 0.0, y: 490.0, width: 300.0, height: 16.0 },
        TextStyle::new(11.0, muted),
    ));

    const K: f32 = 0.5523;
    let (cx1, cy1, r1) = (76.0_f32, 570.0_f32, 44.0_f32);
    let circle_data = Arc::new(
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
        Path::static_data(circle_data.clone(), move || {
            PathStyle::default()
                .with_fill(primary)
                .with_shadow(Shadow::new(4.0, 6.0, 8.0, Color::rgba(0.0, 0.0, 0.0, 0.4)))
        })
        .view(),
    );
    children.push(RenderNode::text(
        rsx::static_rc_str!("drop shadow"),
        Rect { x: 32.0, y: 624.0, width: 88.0, height: 16.0 },
        TextStyle::new(11.0, muted),
    ));

    let star_shadow_data = Arc::new({
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
        Path::static_data(star_shadow_data.clone(), move || {
            PathStyle::default()
                .with_fill(warning)
                .with_shadow(Shadow::new(0.0, 0.0, 10.0, warning.with_alpha(0.7)))
        })
        .view(),
    );
    children.push(RenderNode::text(
        rsx::static_rc_str!("glow"),
        Rect { x: 248.0, y: 624.0, width: 48.0, height: 16.0 },
        TextStyle::new(11.0, muted),
    ));

    let (cx3, cy3, r3) = (468.0_f32, 570.0_f32, 44.0_f32);
    let diamond_data = Arc::new(
        PathData::new()
            .move_to(Point::new(cx3, cy3 - r3))
            .line_to(Point::new(cx3 + r3 * 0.65, cy3))
            .line_to(Point::new(cx3, cy3 + r3))
            .line_to(Point::new(cx3 - r3 * 0.65, cy3))
            .close(),
    );
    children.push(
        Path::static_data(diamond_data.clone(), move || {
            PathStyle::default()
                .with_fill(success)
                .with_shadow(Shadow::new(3.0, 3.0, 2.0, Color::rgba(0.0, 0.0, 0.0, 0.5)))
        })
        .view(),
    );
    children.push(RenderNode::text(
        rsx::static_rc_str!("hard offset"),
        Rect { x: 428.0, y: 624.0, width: 80.0, height: 16.0 },
        TextStyle::new(11.0, muted),
    ));

    let wave_data = Arc::new(
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
        Path::static_data(wave_data.clone(), move || {
            PathStyle::default()
                .with_stroke(Stroke::new(danger, 4.0).with_cap(LineCap::Round))
                .with_shadow(Shadow::new(2.0, 4.0, 6.0, danger.with_alpha(0.5)))
        })
        .view(),
    );
    children.push(RenderNode::text(
        rsx::static_rc_str!("stroke shadow"),
        Rect { x: 600.0, y: 624.0, width: 100.0, height: 16.0 },
        TextStyle::new(11.0, muted),
    ));

    RenderNode::group(children)
})?;

[view]
col gap:8
    text "Paths" size:12 color:muted
    widget "result"
