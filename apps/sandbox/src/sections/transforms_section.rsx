[logic]
use std::sync::Arc;
use crate::theme::theme;

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

let result = Canvas::with_intrinsic_height(ctx, 540.0, |_rect| {
    let t = theme();
    let primary = t.primary;
    let success = t.success;
    let danger = t.danger;
    let warning = t.warning;
    let purple = t.purple;
    let muted = t.muted;

    let mut children: Vec<RenderNode> = Vec::new();

    children.push(RenderNode::text(
        rsx::static_rc_str!("Uniform scale around center — 0.5× / 0.75× / 1× / 1.5× / 2×"),
        Rect { x: 0.0, y: 40.0, width: 600.0, height: 16.0 },
        TextStyle::new(10.5, muted),
    ));

    let base_w = 60.0f32;
    let base_h = 60.0f32;
    let scale_y = 62.0f32;
    let colors = [primary, success, warning, danger, purple];
    let scales = [0.5f32, 0.75, 1.0, 1.5, 2.0];
    let labels = ["0.5×", "0.75×", "1×", "1.5×", "2×"];

    for (i, (&scale, &color)) in scales.iter().zip(colors.iter()).enumerate() {
        let cx = i as f32 * 140.0 + base_w * 0.5;
        let cy = scale_y + base_h * 0.5;
        let matrix = Transform::scale_around(scale, scale, cx, cy).to_array();
        children.push(RenderNode::transform_with(
            matrix,
            [RenderNode::rect(
                Rect { x: cx - base_w * 0.5, y: scale_y, width: base_w, height: base_h },
                RectStyle {
                    fill: Some(Paint::Solid(color)),
                    stroke: None,
                    shadow: None,
                    radius: BorderRadius::all(6.0),
                },
            )],
        ));
        children.push(RenderNode::text(
            labels[i],
            Rect { x: i as f32 * 140.0, y: scale_y + base_h + 4.0, width: 80.0, height: 16.0 },
            TextStyle::new(10.0, muted),
        ));
    }

    children.push(RenderNode::text(
        rsx::static_rc_str!("Rotation — arrow shape at 0° / 30° / 60° / 90° / 120° / 150°"),
        Rect { x: 0.0, y: 178.0, width: 600.0, height: 16.0 },
        TextStyle::new(10.5, muted),
    ));

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
        let matrix = Transform::rotate_around(angle, cx, cy).to_array();
        children.push(RenderNode::transform_with(
            matrix,
            [RenderNode::path(
                Arc::new(path),
                PathStyle {
                    fill: Some(Paint::Solid(color)),
                    stroke: None,
                    shadow: None,
                    fill_rule: FillRule::Winding,
                },
            )],
        ));
        children.push(RenderNode::text(
            *label,
            Rect { x: i as f32 * 110.0 + 10.0, y: rot_y + arrow_size + 6.0, width: 60.0, height: 16.0 },
            TextStyle::new(10.0, muted),
        ));
    }

    children.push(RenderNode::text(
        rsx::static_rc_str!("Combined — rotation then scale (transforms compose)"),
        Rect { x: 0.0, y: 300.0, width: 600.0, height: 16.0 },
        TextStyle::new(10.5, muted),
    ));

    let combo_y = 322.0f32;
    let combo_angles = [0.0f32, 15.0, 30.0, 45.0, 60.0, 75.0, 90.0];
    let combo_scale = [1.0f32, 0.9, 0.78, 0.65, 0.52, 0.4, 0.3];

    for (i, (&angle, &scale)) in combo_angles.iter().zip(combo_scale.iter()).enumerate() {
        let cx = i as f32 * 100.0 + 36.0;
        let cy = combo_y + 36.0;
        // Rotate first, then scale — both around the cell center.
        let matrix = Transform::rotate_around(angle, cx, cy)
            .then(Transform::scale_around(scale, scale, cx, cy))
            .to_array();
        let hue_t = i as f32 / (combo_angles.len() - 1) as f32;
        let color = Color::from_hsl(hue_t * 240.0, 0.7, 0.55);
        children.push(RenderNode::transform_with(
            matrix,
            [RenderNode::rect(
                Rect { x: cx - 28.0, y: cy - 28.0, width: 56.0, height: 56.0 },
                RectStyle {
                    fill: Some(Paint::Solid(color)),
                    stroke: None,
                    shadow: None,
                    radius: BorderRadius::all(8.0),
                },
            )],
        ));
    }

    children.push(RenderNode::text(
        rsx::static_rc_str!("Nested translate + rotation — grid of rotated stroked rects"),
        Rect { x: 0.0, y: 430.0, width: 600.0, height: 16.0 },
        TextStyle::new(10.5, muted),
    ));

    let grid_y = 452.0f32;
    for row in 0..2i32 {
        for col in 0..7i32 {
            let cx = col as f32 * 90.0 + 30.0;
            let cy = grid_y + row as f32 * 60.0 + 24.0;
            let angle = (row * 7 + col) as f32 * 12.0;
            let matrix = Transform::rotate_around(angle, cx, cy).to_array();
            children.push(RenderNode::transform_with(
                matrix,
                [RenderNode::rect(
                    Rect { x: cx - 22.0, y: cy - 18.0, width: 44.0, height: 36.0 },
                    RectStyle {
                        fill: Some(Paint::Solid(Color::from_hsla(angle % 360.0, 0.65, 0.55, 0.18))),
                        stroke: Some(Stroke::new(Color::from_hsl(angle % 360.0, 0.65, 0.55), 2.0)),
                        shadow: None,
                        radius: BorderRadius::all(4.0),
                    },
                )],
            ));
        }
    }

    RenderNode::group(children)
})?;

[view]
col gap:8
    text "Transforms" size:12 color:muted
    widget "result"
