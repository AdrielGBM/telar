[logic]
use crate::core::theme::theme;
use std::sync::Arc;

// Paths are drawn imperatively inside a Canvas — the escape hatch for vector art layout can't express.
// Widest x reached by the fixed-coordinate art below; the drawing is scaled to fit narrower canvases.
const PATHS_DESIGN_W: f32 = 520.0;

let result = Canvas::with_intrinsic_height(ctx, 430.0, |rect| {
    let t = theme();
    let (primary, success, danger, warning, purple, ink, muted) = (
        t.primary, t.success, t.danger, t.warning, t.purple, t.ink, t.muted,
    );
    let mut kids: Vec<RenderNode> = Vec::new();

    // Row 1 — filled and stroked polygons.
    let triangle = Arc::new(PathData::polygon(&[
        Point::new(60.0, 30.0),
        Point::new(110.0, 120.0),
        Point::new(10.0, 120.0),
    ]));
    kids.push(
        Path::static_data(triangle, move || PathStyle {
            fill: Some(Paint::Solid(primary)),
            stroke: None,
            shadow: None,
            fill_rule: FillRule::Winding,
        })
        .view(),
    );
    kids.push(RenderNode::text(
        rsx::static_rc_str!("polygon fill"),
        Rect {
            x: 0.0,
            y: 128.0,
            width: 160.0,
            height: 14.0,
        },
        TextStyle::new(11.0, muted),
    ));

    let star = {
        let (cx, cy, outer, inner) = (230.0f32, 76.0f32, 48.0f32, 20.0f32);
        let pts: Vec<Point> = (0..10)
            .map(|i| {
                let a = std::f32::consts::TAU * i as f32 / 10.0 - std::f32::consts::FRAC_PI_2;
                let r = if i % 2 == 0 { outer } else { inner };
                Point::new(cx + r * a.cos(), cy + r * a.sin())
            })
            .collect();
        Arc::new(PathData::polygon(&pts))
    };
    kids.push(
        Path::static_data(star, move || PathStyle {
            fill: Some(Paint::Solid(warning)),
            stroke: Some(Stroke::new(ink, 1.5)),
            shadow: None,
            fill_rule: FillRule::Winding,
        })
        .view(),
    );
    kids.push(RenderNode::text(
        rsx::static_rc_str!("fill + stroke"),
        Rect {
            x: 175.0,
            y: 128.0,
            width: 160.0,
            height: 14.0,
        },
        TextStyle::new(11.0, muted),
    ));

    // Even-odd fill carves a hole where two rectangles overlap.
    let donut = Arc::new(
        PathData::new()
            .move_to(Point::new(360.0, 30.0))
            .line_to(Point::new(460.0, 30.0))
            .line_to(Point::new(460.0, 120.0))
            .line_to(Point::new(360.0, 120.0))
            .close()
            .move_to(Point::new(385.0, 55.0))
            .line_to(Point::new(435.0, 55.0))
            .line_to(Point::new(435.0, 95.0))
            .line_to(Point::new(385.0, 95.0))
            .close(),
    );
    kids.push(
        Path::static_data(donut, move || PathStyle {
            fill: Some(Paint::Solid(purple)),
            stroke: None,
            shadow: None,
            fill_rule: FillRule::EvenOdd,
        })
        .view(),
    );
    kids.push(RenderNode::text(
        rsx::static_rc_str!("even-odd fill"),
        Rect {
            x: 350.0,
            y: 128.0,
            width: 160.0,
            height: 14.0,
        },
        TextStyle::new(11.0, muted),
    ));

    // Row 2 — Bézier curves.
    let quad = Arc::new(
        PathData::new()
            .move_to(Point::new(10.0, 250.0))
            .quad_to(Point::new(80.0, 180.0), Point::new(150.0, 250.0)),
    );
    kids.push(
        Path::static_data(quad, move || PathStyle {
            fill: None,
            stroke: Some(Stroke::new(success, 3.0).with_cap(LineCap::Round)),
            shadow: None,
            fill_rule: FillRule::Winding,
        })
        .view(),
    );
    kids.push(RenderNode::text(
        rsx::static_rc_str!("quad_to arch"),
        Rect {
            x: 0.0,
            y: 258.0,
            width: 160.0,
            height: 14.0,
        },
        TextStyle::new(11.0, muted),
    ));

    let cubic = Arc::new(PathData::new().move_to(Point::new(185.0, 200.0)).cubic_to(
        Point::new(260.0, 200.0),
        Point::new(185.0, 250.0),
        Point::new(260.0, 250.0),
    ));
    kids.push(
        Path::static_data(cubic, move || PathStyle {
            fill: None,
            stroke: Some(Stroke::new(danger, 3.0).with_cap(LineCap::Round)),
            shadow: None,
            fill_rule: FillRule::Winding,
        })
        .view(),
    );
    kids.push(RenderNode::text(
        rsx::static_rc_str!("cubic_to S-curve"),
        Rect {
            x: 175.0,
            y: 258.0,
            width: 160.0,
            height: 14.0,
        },
        TextStyle::new(11.0, muted),
    ));

    // A drop shadow under a hand-rolled circle.
    let k = 0.5523f32;
    let (cx, cy, r) = (410.0f32, 220.0f32, 34.0f32);
    let circle = Arc::new(
        PathData::new()
            .move_to(Point::new(cx, cy - r))
            .cubic_to(
                Point::new(cx + k * r, cy - r),
                Point::new(cx + r, cy - k * r),
                Point::new(cx + r, cy),
            )
            .cubic_to(
                Point::new(cx + r, cy + k * r),
                Point::new(cx + k * r, cy + r),
                Point::new(cx, cy + r),
            )
            .cubic_to(
                Point::new(cx - k * r, cy + r),
                Point::new(cx - r, cy + k * r),
                Point::new(cx - r, cy),
            )
            .cubic_to(
                Point::new(cx - r, cy - k * r),
                Point::new(cx - k * r, cy - r),
                Point::new(cx, cy - r),
            )
            .close(),
    );
    kids.push(
        Path::static_data(circle, move || {
            PathStyle::default()
                .with_fill(primary)
                .with_shadow(Shadow::new(3.0, 5.0, 8.0, Color::rgba(0.0, 0.0, 0.0, 0.4)))
        })
        .view(),
    );
    kids.push(RenderNode::text(
        rsx::static_rc_str!("path + drop shadow"),
        Rect {
            x: 350.0,
            y: 268.0,
            width: 170.0,
            height: 14.0,
        },
        TextStyle::new(11.0, muted),
    ));

    let scale = (rect.width / PATHS_DESIGN_W).min(1.0);
    RenderNode::transform_with([scale, 0.0, 0.0, scale, 0.0, 0.0], kids)
})?;

[view]
col gap:20
    doc_header kicker:"11 · MEDIA" title:"Paths" desc:"Build vector geometry with PathData — lines, quadratic and cubic Béziers, winding vs even-odd fills, stroke caps, and per-path shadows — then draw it in a Canvas."
    col gap:8
        text "Polygons, curves, fills and a path shadow" size:13 color:ink
        card
            widget "result"
        code_line code:"PathData::new().move_to(p).cubic_to(a, b, c)   >   Path::static_data(d, style)"
    col gap:8
        text "Declarative paths in [view]" size:13 color:ink
        card
            row gap:28 align:center
                path d:"M0,0 L100,0 L50,80 Z" fill:primary stroke:ink stroke_width:2 width:100 height:80
                path d:"M6,42 L34,70 L74,14" stroke:success stroke_width:7 width:80 height:80
                path d:"M40,2 L50,30 L80,30 L56,48 L64,78 L40,60 L16,78 L24,48 L0,30 L30,30 Z" fill:warning stroke:ink stroke_width:1 width:80 height:80
        code_line code:"path d:\"M0,0 L100,0 L50,80 Z\" fill:primary stroke:ink stroke_width:2 width:100 height:80"
    col gap:8
        text "The PathData API" size:13 color:ink
        col gap:6
            prop_row name:"move_to / line_to" values:"Point" about:"Start a subpath, add a straight segment."
            prop_row name:"quad_to / cubic_to" values:"Points" about:"Quadratic and cubic Bézier curves."
            prop_row name:"fill_rule" values:"Winding · EvenOdd" about:"How overlapping regions are filled."
            prop_row name:"Stroke::with_cap" values:"Butt·Round·Square" about:"Line ends (and with_join for corners)."
