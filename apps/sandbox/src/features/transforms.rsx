[logic]
use crate::core::theme::theme;
use std::sync::Arc;

fn arrow(cx: f32, cy: f32, size: f32) -> PathData {
    let hs = size * 0.5;
    let head = size * 0.35;
    let sh = size * 0.18;
    PathData::new()
        .move_to(Point::new(cx - hs, cy - sh))
        .line_to(Point::new(cx + hs - head, cy - sh))
        .line_to(Point::new(cx + hs - head, cy - hs * 0.5))
        .line_to(Point::new(cx + hs, cy))
        .line_to(Point::new(cx + hs - head, cy + hs * 0.5))
        .line_to(Point::new(cx + hs - head, cy + sh))
        .line_to(Point::new(cx - hs, cy + sh))
        .close()
}

// Widest x reached by the fixed-coordinate cells below; the drawing is scaled to fit narrower canvases.
const TRANSFORMS_DESIGN_W: f32 = 640.0;

let result = Canvas::with_intrinsic_height(ctx, 360.0, |rect| {
    let t = theme();
    let muted = t.muted;
    let palette = [t.primary, t.success, t.warning, t.danger, t.purple];
    let mut kids: Vec<RenderNode> = Vec::new();

    // Uniform scale around each cell's center.
    kids.push(RenderNode::text(rsx::static_rc_str!("scale around center — 0.5× · 0.75× · 1× · 1.5× · 2×"), Rect { x: 0.0, y: 8.0, width: 640.0, height: 14.0 }, TextStyle::new(11.0, muted)));
    let scales = [0.5f32, 0.75, 1.0, 1.5, 2.0];
    for (i, (&s, &color)) in scales.iter().zip(palette.iter()).enumerate() {
        let cx = i as f32 * 130.0 + 40.0;
        let cy = 70.0f32;
        let m = Transform::scale_around(s, s, cx, cy).to_array();
        kids.push(RenderNode::transform_with(m, [RenderNode::rect(
            Rect { x: cx - 26.0, y: cy - 26.0, width: 52.0, height: 52.0 },
            RectStyle { fill: Some(Paint::Solid(color)), stroke: None, shadow: None, radius: BorderRadius::all(6.0) },
        )]));
    }

    // Rotation of a shared arrow shape.
    kids.push(RenderNode::text(rsx::static_rc_str!("rotate around center — 0° · 30° · 60° · 90° · 120°"), Rect { x: 0.0, y: 140.0, width: 640.0, height: 14.0 }, TextStyle::new(11.0, muted)));
    let angles = [0.0f32, 30.0, 60.0, 90.0, 120.0];
    for (i, (&a, &color)) in angles.iter().zip(palette.iter()).enumerate() {
        let cx = i as f32 * 130.0 + 45.0;
        let cy = 200.0f32;
        let m = Transform::rotate_around(a, cx, cy).to_array();
        kids.push(RenderNode::transform_with(m, [RenderNode::path(
            Arc::new(arrow(cx, cy, 54.0)),
            PathStyle { fill: Some(Paint::Solid(color)), stroke: None, shadow: None, fill_rule: FillRule::Winding },
        )]));
    }

    // Compose: rotate then scale, both around the same center.
    kids.push(RenderNode::text(rsx::static_rc_str!("compose — rotate then scale (transforms multiply)"), Rect { x: 0.0, y: 270.0, width: 640.0, height: 14.0 }, TextStyle::new(11.0, muted)));
    let steps = [0.0f32, 20.0, 40.0, 60.0, 80.0];
    for (i, &a) in steps.iter().enumerate() {
        let cx = i as f32 * 130.0 + 40.0;
        let cy = 325.0f32;
        let s = 1.0 - i as f32 * 0.16;
        let m = Transform::rotate_around(a, cx, cy)
            .then(Transform::scale_around(s, s, cx, cy))
            .to_array();
        kids.push(RenderNode::transform_with(m, [RenderNode::rect(
            Rect { x: cx - 26.0, y: cy - 26.0, width: 52.0, height: 52.0 },
            RectStyle { fill: Some(Paint::Solid(palette[i % palette.len()])), stroke: None, shadow: None, radius: BorderRadius::all(8.0) },
        )]));
    }

    let scale = (rect.width / TRANSFORMS_DESIGN_W).min(1.0);
    RenderNode::transform_with([scale, 0.0, 0.0, scale, 0.0, 0.0], kids)
})?;

[view]
col gap:20
    doc_header kicker:"12 · MEDIA" title:"Transforms" desc:"Wrap any render node in an affine matrix to scale, rotate, or translate it. Transforms compose with .then(), so you can rotate and then scale around the same point."

    col gap:8
        text "Scale, rotate, and a composed rotate-then-scale" size:13 color:ink
        card
            widget "result"
        code_line code:"Transform::rotate_around(a, cx, cy).then(Transform::scale_around(s, s, cx, cy))"

    col gap:8
        text "Declarative — rotate / scale / translate as box attributes (no Canvas, no Rust)" size:13 color:ink
        card
            row gap:24 justify:center pad_y:16
                box fill:primary radius:8 width:56 height:56
                box fill:success radius:8 width:56 height:56 rotate:20
                box fill:warning radius:8 width:56 height:56 scale:1.3
                box fill:danger radius:8 width:56 height:56 rotate:15 scale:0.85
                box fill:ink radius:8 width:56 height:56 rotate:-12 translate_y:-8
        code_line code:"box fill:success rotate:20      box fill:danger rotate:15 scale:0.85"

    col gap:8
        text "The Transform API" size:13 color:ink
        col gap:6
            prop_row name:"scale_around" values:"sx, sy, cx, cy" about:"Scale about a pivot point."
            prop_row name:"rotate_around" values:"deg, cx, cy" about:"Rotate about a pivot point."
            prop_row name:".then(other)" values:"Transform" about:"Compose two transforms into one matrix."
            prop_row name:"transform_with" values:"matrix, [nodes]" about:"Apply a matrix to child render nodes."
