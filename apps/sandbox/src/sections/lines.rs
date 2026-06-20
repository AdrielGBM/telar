use std::sync::Arc;

use rsx::{
    Canvas, Color, Component, LayoutError, Line, LineCap, LineStyle, Point, Rect, RenderNode,
    TextStyle, WidgetCtx,
};

use crate::theme::{draw_section_header, theme};

pub fn lines_section(ctx: &mut WidgetCtx) -> Result<Canvas, LayoutError> {
    Canvas::with_intrinsic_height(ctx, 330.0, |rect| {
        let w = rect.width;
        let t = theme();
        let primary = t.primary;
        let success = t.success;
        let danger = t.danger;
        let warning = t.warning;
        let purple = t.purple;
        let dark = t.dark;
        let muted = t.muted;
        let card_border = t.card_border;

        let mut children: Vec<RenderNode> = Vec::new();

        draw_section_header(&mut children, w, "Lines");

        children.push(RenderNode::text(
            crate::static_rc_str!("Width"),
            Rect {
                x: 0.0,
                y: 40.0,
                width: 60.0,
                height: 16.0,
            },
            TextStyle::new(11.0, muted),
        ));

        let width_examples: &[(f32, &str)] = &[
            (1.0, "1 px"),
            (2.0, "2 px"),
            (4.0, "4 px"),
            (8.0, "8 px"),
            (16.0, "16 px"),
        ];
        let mut cy = 62.0f32;
        for &(w, label) in width_examples {
            children.push(RenderNode::text(
                Arc::<str>::from(label),
                Rect {
                    x: 0.0,
                    y: cy - 8.0,
                    width: 56.0,
                    height: 16.0,
                },
                TextStyle::new(11.0, muted),
            ));
            children.push(
                Line::new(
                    move || Point::new(64.0, cy),
                    move || Point::new(336.0, cy),
                    move || LineStyle::new(primary, w),
                )
                .view(),
            );
            cy += w.max(2.0) + 18.0;
        }

        children.push(RenderNode::text(
            crate::static_rc_str!("Color"),
            Rect {
                x: 396.0,
                y: 40.0,
                width: 60.0,
                height: 16.0,
            },
            TextStyle::new(11.0, muted),
        ));
        let color_examples: &[(Color, &str)] = &[
            (primary, "primary"),
            (success, "success"),
            (danger, "danger"),
            (warning, "warning"),
            (purple, "purple"),
        ];
        for (i, &(color, label)) in color_examples.iter().enumerate() {
            let y = 62.0 + i as f32 * 24.0;
            children.push(
                Line::new(
                    move || Point::new(396.0, y),
                    move || Point::new(656.0, y),
                    move || LineStyle::new(color, 3.0),
                )
                .view(),
            );
            children.push(RenderNode::text(
                Arc::<str>::from(label),
                Rect {
                    x: 664.0,
                    y: y - 8.0,
                    width: 80.0,
                    height: 16.0,
                },
                TextStyle::new(11.0, color),
            ));
        }

        children.push(RenderNode::text(
            crate::static_rc_str!("Separator & chart"),
            Rect {
                x: 0.0,
                y: 176.0,
                width: 300.0,
                height: 16.0,
            },
            TextStyle::new(11.0, muted),
        ));
        children.push(
            Line::new(
                || Point::new(0.0, 196.0),
                move || Point::new(w, 196.0),
                move || LineStyle::new(card_border, 1.0),
            )
            .view(),
        );

        let ax = 36.0f32;
        let cb = 306.0f32;
        let ct = 216.0f32;
        let ax_right = 376.0f32;
        children.push(
            Line::new(
                move || Point::new(ax, ct),
                move || Point::new(ax, cb),
                move || LineStyle::new(muted, 1.0),
            )
            .view(),
        );
        children.push(
            Line::new(
                move || Point::new(ax, cb),
                move || Point::new(ax_right, cb),
                move || LineStyle::new(muted, 1.0),
            )
            .view(),
        );

        let data_x = [ax, ax + 85.0, ax + 170.0, ax + 255.0, ax_right];
        let s1 = [296.0f32, 271.0, 254.0, 234.0, 224.0];
        let s2 = [291.0f32, 268.0, 256.0, 244.0, 231.0];
        let s3 = [271.0f32, 278.0, 286.0, 294.0, 301.0];
        for i in 0..4 {
            children.push(
                Line::new(
                    move || Point::new(data_x[i], s1[i]),
                    move || Point::new(data_x[i + 1], s1[i + 1]),
                    move || LineStyle::new(primary, 2.0),
                )
                .view(),
            );
            children.push(
                Line::new(
                    move || Point::new(data_x[i], s2[i]),
                    move || Point::new(data_x[i + 1], s2[i + 1]),
                    move || LineStyle::new(success, 2.0),
                )
                .view(),
            );
            children.push(
                Line::new(
                    move || Point::new(data_x[i], s3[i]),
                    move || Point::new(data_x[i + 1], s3[i + 1]),
                    move || LineStyle::new(danger, 2.0),
                )
                .view(),
            );
        }

        children.push(RenderNode::text(
            crate::static_rc_str!("Diagonals"),
            Rect {
                x: 436.0,
                y: 200.0,
                width: 120.0,
                height: 16.0,
            },
            TextStyle::new(11.0, muted),
        ));
        let fan_cx = 566.0f32;
        let fan_cy = 266.0f32;
        let fan_tips: &[(f32, f32, Color)] = &[
            (476.0, 220.0, primary),
            (516.0, 214.0, success),
            (566.0, 214.0, danger),
            (616.0, 214.0, warning),
            (656.0, 220.0, purple),
            (686.0, 238.0, dark),
        ];
        for &(tx, ty, color) in fan_tips {
            children.push(
                Line::new(
                    move || Point::new(fan_cx, fan_cy),
                    move || Point::new(tx, ty),
                    move || LineStyle::new(color, 2.0).with_cap(LineCap::Round),
                )
                .view(),
            );
        }

        RenderNode::group(children)
    })
}
