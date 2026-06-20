use std::sync::Arc;

use rsx::{
    BorderRadius, Canvas, Color, Gradient, LayoutError, Paint, Point, Rect, RectStyle, RenderNode,
    TextStyle, WidgetCtx,
};

use crate::theme::{draw_section_header, theme};

pub fn layers_section(ctx: &mut WidgetCtx) -> Result<Canvas, LayoutError> {
    Canvas::with_intrinsic_height(ctx, 560.0, |rect| {
        let w = rect.width;
        let t = theme();
        let primary = t.primary;
        let success = t.success;
        let danger = t.danger;
        let purple = t.purple;
        let dark = t.dark;
        let muted = t.muted;

        let mut children: Vec<RenderNode> = Vec::new();

        draw_section_header(&mut children, w, "Layers (PushLayer / PopLayer)");

        children.push(RenderNode::text(
            crate::static_rc_str!("Opacity — same red rect at 1.0 / 0.6 / 0.3 / 0.1"),
            Rect {
                x: 0.0,
                y: 40.0,
                width: 500.0,
                height: 16.0,
            },
            TextStyle::new(11.0, muted),
        ));

        for (i, &opacity) in [1.0f32, 0.6, 0.3, 0.1].iter().enumerate() {
            let x = i as f32 * 184.0;
            children.push(RenderNode::layer(
                opacity,
                0.0,
                [
                    RenderNode::rect(
                        Rect {
                            x,
                            y: 60.0,
                            width: 168.0,
                            height: 80.0,
                        },
                        RectStyle {
                            fill: Some(Paint::Solid(danger)),
                            stroke: None,
                            shadow: None,
                            radius: BorderRadius::all(8.0),
                        },
                    ),
                    RenderNode::text(
                        Arc::<str>::from(format!("{opacity:.1}")),
                        Rect {
                            x,
                            y: 64.0,
                            width: 168.0,
                            height: 72.0,
                        },
                        TextStyle::new(18.0, Color::WHITE),
                    ),
                ],
            ));
        }

        children.push(RenderNode::text(
            crate::static_rc_str!("Overlapping colored layers at 0.7 opacity"),
            Rect {
                x: 0.0,
                y: 164.0,
                width: 500.0,
                height: 16.0,
            },
            TextStyle::new(11.0, muted),
        ));

        children.push(RenderNode::rect(
            Rect {
                x: 0.0,
                y: 184.0,
                width: 368.0,
                height: 180.0,
            },
            RectStyle {
                fill: Some(Paint::Solid(dark)),
                stroke: None,
                shadow: None,
                radius: BorderRadius::all(8.0),
            },
        ));

        children.push(RenderNode::layer(
            0.7,
            0.0,
            [RenderNode::rect(
                Rect {
                    x: 16.0,
                    y: 200.0,
                    width: 180.0,
                    height: 120.0,
                },
                RectStyle {
                    fill: Some(Paint::Solid(primary)),
                    stroke: None,
                    shadow: None,
                    radius: BorderRadius::all(8.0),
                },
            )],
        ));

        children.push(RenderNode::layer(
            0.7,
            0.0,
            [RenderNode::rect(
                Rect {
                    x: 96.0,
                    y: 240.0,
                    width: 180.0,
                    height: 120.0,
                },
                RectStyle {
                    fill: Some(Paint::Solid(success)),
                    stroke: None,
                    shadow: None,
                    radius: BorderRadius::all(8.0),
                },
            )],
        ));

        children.push(RenderNode::layer(
            0.7,
            0.0,
            [RenderNode::rect(
                Rect {
                    x: 176.0,
                    y: 220.0,
                    width: 180.0,
                    height: 120.0,
                },
                RectStyle {
                    fill: Some(Paint::Solid(danger)),
                    stroke: None,
                    shadow: None,
                    radius: BorderRadius::all(8.0),
                },
            )],
        ));

        children.push(RenderNode::text(
            crate::static_rc_str!("Layer (0.8) wrapping a gradient rect + text"),
            Rect {
                x: 396.0,
                y: 164.0,
                width: 360.0,
                height: 16.0,
            },
            TextStyle::new(11.0, muted),
        ));

        children.push(RenderNode::layer(
            0.8,
            0.0,
            [
                RenderNode::rect(
                    Rect {
                        x: 396.0,
                        y: 184.0,
                        width: 320.0,
                        height: 180.0,
                    },
                    RectStyle {
                        fill: Some(Paint::Gradient(Gradient::linear(
                            Point::new(396.0, 274.0),
                            Point::new(716.0, 274.0),
                            &[(0.0, primary), (0.5, purple), (1.0, danger)],
                        ))),
                        stroke: None,
                        shadow: None,
                        radius: BorderRadius::all(12.0),
                    },
                ),
                RenderNode::text(
                    crate::static_rc_str!("gradient + layer"),
                    Rect {
                        x: 396.0,
                        y: 254.0,
                        width: 320.0,
                        height: 60.0,
                    },
                    TextStyle::new(18.0, Color::WHITE),
                ),
            ],
        ));

        children.push(RenderNode::text(
            crate::static_rc_str!("Nested layers: outer 0.6, inner 0.5 → combined ~0.3"),
            Rect {
                x: 0.0,
                y: 390.0,
                width: 500.0,
                height: 16.0,
            },
            TextStyle::new(11.0, muted),
        ));

        children.push(RenderNode::layer(
            0.6,
            0.0,
            [
                RenderNode::rect(
                    Rect {
                        x: 0.0,
                        y: 410.0,
                        width: 340.0,
                        height: 120.0,
                    },
                    RectStyle {
                        fill: Some(Paint::Solid(primary)),
                        stroke: None,
                        shadow: None,
                        radius: BorderRadius::all(8.0),
                    },
                ),
                RenderNode::layer(
                    0.5,
                    0.0,
                    [
                        RenderNode::rect(
                            Rect {
                                x: 36.0,
                                y: 430.0,
                                width: 260.0,
                                height: 80.0,
                            },
                            RectStyle {
                                fill: Some(Paint::Solid(danger)),
                                stroke: None,
                                shadow: None,
                                radius: BorderRadius::all(6.0),
                            },
                        ),
                        RenderNode::text(
                            crate::static_rc_str!("inner 0.5"),
                            Rect {
                                x: 36.0,
                                y: 434.0,
                                width: 260.0,
                                height: 72.0,
                            },
                            TextStyle::new(14.0, Color::WHITE),
                        ),
                    ],
                ),
                RenderNode::text(
                    crate::static_rc_str!("outer 0.6"),
                    Rect {
                        x: 0.0,
                        y: 414.0,
                        width: 340.0,
                        height: 20.0,
                    },
                    TextStyle::new(11.0, Color::rgba(1.0, 1.0, 1.0, 0.7)),
                ),
            ],
        ));

        RenderNode::group(children)
    })
}
