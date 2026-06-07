use std::rc::Rc;

use rsx::{
    BorderRadius, Color, DrawCommand, DrawingArea, LayoutError, LayoutStyle, Paint, PathData,
    PathStyle, Point, Rect, RectPayload, RectStyle, RenderNode, Size, Stroke, TextPayload,
    TextStyle, WidgetCtx, use_theme,
};

use crate::sections::draw_section_header;
use crate::theme::SandboxTheme;

fn rotation_matrix(angle_deg: f32, cx: f32, cy: f32) -> [f32; 6] {
    let a = angle_deg.to_radians();
    let cos = a.cos();
    let sin = a.sin();
    [
        cos,
        sin,
        -sin,
        cos,
        cx - cx * cos + cy * sin,
        cy - cx * sin - cy * cos,
    ]
}

fn scale_matrix(sx: f32, sy: f32, cx: f32, cy: f32) -> [f32; 6] {
    [sx, 0.0, 0.0, sy, cx - sx * cx, cy - sy * cy]
}

fn rect_view(x: f32, y: f32, w: f32, h: f32, fill: Color, radius: f32) -> RenderNode {
    RenderNode::Primitive(DrawCommand::Rect(Box::new(RectPayload {
        rect: Rect {
            x,
            y,
            width: w,
            height: h,
        },
        style: RectStyle {
            fill: Some(Paint::Solid(fill)),
            stroke: None,
            shadow: None,
            radius: BorderRadius::all(radius),
        },
    })))
}

fn label_view(text: &'static str, x: f32, y: f32, w: f32, color: Color) -> RenderNode {
    RenderNode::Primitive(DrawCommand::Text(Box::new(TextPayload {
        text: Rc::from(text),
        rect: Rect {
            x,
            y,
            width: w,
            height: 16.0,
        },
        style: TextStyle::new(10.0, color),
    })))
}

fn arrow_path(cx: f32, cy: f32, size: f32) -> PathData {
    let hs = size * 0.5;
    let head = size * 0.35;
    let shaft_h = size * 0.18;
    PathData::new()
        .move_to(Point::new(cx - hs, cy - shaft_h))
        .line_to(Point::new(cx + hs - head, cy - shaft_h))
        .line_to(Point::new(cx + hs - head, cy - hs * 0.5))
        .line_to(Point::new(cx + hs, cy))
        .line_to(Point::new(cx + hs - head, cy + hs * 0.5))
        .line_to(Point::new(cx + hs - head, cy + shaft_h))
        .line_to(Point::new(cx - hs, cy + shaft_h))
        .close()
}

pub fn transforms_section(ctx: &mut WidgetCtx) -> Result<DrawingArea, LayoutError> {
    DrawingArea::new(
        ctx,
        LayoutStyle::new().height(580.0),
        |Size { width: w, .. }| {
            let t = use_theme::<SandboxTheme>();
            let primary = t.primary;
            let success = t.success;
            let danger = t.danger;
            let warning = t.warning;
            let purple = t.purple;
            let muted = t.muted;
            let card_border = t.card_border;

            let mut children: Vec<RenderNode> = Vec::new();

            // — section divider & title —
            draw_section_header(
                &mut children,
                w,
                "Transforms (PushMatrix / PopMatrix)",
                card_border,
                muted,
            );

            // ── Scale ──────────────────────────────────────────────────────────
            children.push(RenderNode::Primitive(DrawCommand::Text(Box::new(
                TextPayload {
                    text: Rc::from("Uniform scale around center — 0.5× / 0.75× / 1× / 1.5× / 2×"),
                    rect: Rect {
                        x: 0.0,
                        y: 40.0,
                        width: 600.0,
                        height: 16.0,
                    },
                    style: TextStyle::new(10.5, muted),
                },
            ))));

            let base_w = 60.0f32;
            let base_h = 60.0f32;
            let scale_y = 62.0f32;
            let colors = [primary, success, warning, danger, purple];
            let scales = [0.5f32, 0.75, 1.0, 1.5, 2.0];
            let labels = ["0.5×", "0.75×", "1×", "1.5×", "2×"];

            for (i, (&scale, &color)) in scales.iter().zip(colors.iter()).enumerate() {
                let cx = i as f32 * 140.0 + base_w * 0.5;
                let cy = scale_y + base_h * 0.5;
                let matrix = scale_matrix(scale, scale, cx, cy);
                children.push(RenderNode::Transform {
                    matrix,
                    children: vec![rect_view(
                        cx - base_w * 0.5,
                        scale_y,
                        base_w,
                        base_h,
                        color,
                        6.0,
                    )],
                });
                children.push(label_view(
                    labels[i],
                    i as f32 * 140.0,
                    scale_y + base_h + 4.0,
                    80.0,
                    muted,
                ));
            }

            // ── Rotation ───────────────────────────────────────────────────────
            children.push(RenderNode::Primitive(DrawCommand::Text(Box::new(
                TextPayload {
                    text: Rc::from("Rotation — arrow shape at 0° / 30° / 60° / 90° / 120° / 150°"),
                    rect: Rect {
                        x: 0.0,
                        y: 178.0,
                        width: 600.0,
                        height: 16.0,
                    },
                    style: TextStyle::new(10.5, muted),
                },
            ))));

            let rot_y = 200.0f32;
            let arrow_size = 52.0f32;
            let rot_angles = [0.0f32, 30.0, 60.0, 90.0, 120.0, 150.0];
            let rot_labels = ["0°", "30°", "60°", "90°", "120°", "150°"];
            let rot_colors = [
                primary,
                success,
                warning,
                danger,
                purple,
                Color::from_rgb_u8(20, 180, 210),
            ];

            for (i, ((&angle, &color), label)) in rot_angles
                .iter()
                .zip(rot_colors.iter())
                .zip(rot_labels.iter())
                .enumerate()
            {
                let cx = i as f32 * 110.0 + arrow_size * 0.5 + 10.0;
                let cy = rot_y + arrow_size * 0.5;
                let path = arrow_path(cx, cy, arrow_size);
                let matrix = rotation_matrix(angle, cx, cy);
                children.push(RenderNode::Transform {
                    matrix,
                    children: vec![RenderNode::Primitive(DrawCommand::Path(Box::new(
                        rsx::PathPayload {
                            data: Rc::new(path),
                            style: PathStyle {
                                fill: Some(Paint::Solid(color)),
                                stroke: None,
                                shadow: None,
                                fill_rule: rsx::FillRule::Winding,
                            },
                        },
                    )))],
                });
                children.push(label_view(
                    label,
                    i as f32 * 110.0 + 10.0,
                    rot_y + arrow_size + 6.0,
                    60.0,
                    muted,
                ));
            }

            // ── Scale + Rotation combined ──────────────────────────────────────
            children.push(RenderNode::Primitive(DrawCommand::Text(Box::new(
                TextPayload {
                    text: Rc::from("Combined — rotation then scale (transforms compose)"),
                    rect: Rect {
                        x: 0.0,
                        y: 300.0,
                        width: 600.0,
                        height: 16.0,
                    },
                    style: TextStyle::new(10.5, muted),
                },
            ))));

            let combo_y = 322.0f32;
            let combo_angles = [0.0f32, 15.0, 30.0, 45.0, 60.0, 75.0, 90.0];
            let combo_scale = [1.0f32, 0.9, 0.78, 0.65, 0.52, 0.4, 0.3];

            for (i, (&angle, &scale)) in combo_angles.iter().zip(combo_scale.iter()).enumerate() {
                let cx = i as f32 * 100.0 + 36.0;
                let cy = combo_y + 36.0;
                let rot = rotation_matrix(angle, cx, cy);
                let scl = scale_matrix(scale, scale, cx, cy);
                // Compose: apply scale after rotation — scl composed with rot.
                let [a1, b1, c1, d1, e1, f1] = rot;
                let [a2, b2, c2, d2, e2, f2] = scl;
                let combined = [
                    a2 * a1 + c2 * b1,
                    b2 * a1 + d2 * b1,
                    a2 * c1 + c2 * d1,
                    b2 * c1 + d2 * d1,
                    a2 * e1 + c2 * f1 + e2,
                    b2 * e1 + d2 * f1 + f2,
                ];
                let hue_t = i as f32 / (combo_angles.len() - 1) as f32;
                let color = Color::from_hsl(hue_t * 240.0, 0.7, 0.55);
                children.push(RenderNode::Transform {
                    matrix: combined,
                    children: vec![rect_view(cx - 28.0, cy - 28.0, 56.0, 56.0, color, 8.0)],
                });
            }

            // ── Stroke rect with rotation ──────────────────────────────────────
            children.push(RenderNode::Primitive(DrawCommand::Text(Box::new(
                TextPayload {
                    text: Rc::from("Nested translate + rotation — grid of rotated stroked rects"),
                    rect: Rect {
                        x: 0.0,
                        y: 430.0,
                        width: 600.0,
                        height: 16.0,
                    },
                    style: TextStyle::new(10.5, muted),
                },
            ))));

            let grid_y = 452.0f32;
            for row in 0..2i32 {
                for col in 0..7i32 {
                    let cx = col as f32 * 90.0 + 30.0;
                    let cy = grid_y + row as f32 * 60.0 + 24.0;
                    let angle = (row * 7 + col) as f32 * 12.0;
                    let matrix = rotation_matrix(angle, cx, cy);
                    children.push(RenderNode::Transform {
                        matrix,
                        children: vec![RenderNode::Primitive(DrawCommand::Rect(Box::new(
                            RectPayload {
                                rect: Rect {
                                    x: cx - 22.0,
                                    y: cy - 18.0,
                                    width: 44.0,
                                    height: 36.0,
                                },
                                style: RectStyle {
                                    fill: Some(Paint::Solid(Color::from_hsla(
                                        angle % 360.0,
                                        0.65,
                                        0.55,
                                        0.18,
                                    ))),
                                    stroke: Some(Stroke::new(
                                        Color::from_hsl(angle % 360.0, 0.65, 0.55),
                                        2.0,
                                    )),
                                    shadow: None,
                                    radius: BorderRadius::all(4.0),
                                },
                            },
                        )))],
                    });
                }
            }

            RenderNode::group(children)
        },
    )
}
