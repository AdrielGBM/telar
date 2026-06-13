use std::sync::Arc;

use rsx::{
    BorderRadius, Canvas, Color, DrawCommand, Gradient, LayoutError, NodeVec, Paint, Point, Rect,
    RectPayload, RectStyle, RenderNode, TextPayload, TextStyle, WidgetCtx, use_theme,
};

use crate::sections::draw_section_header;
use crate::theme::SandboxTheme;

pub fn layers_section(ctx: &mut WidgetCtx) -> Result<Canvas, LayoutError> {
    Canvas::with_intrinsic_height(ctx, 560.0, |rect| {
        let w = rect.width;
        let t = use_theme::<SandboxTheme>();
        let primary = t.primary;
        let success = t.success;
        let danger = t.danger;
        let purple = t.purple;
        let dark = t.dark;
        let muted = t.muted;
        let card_border = t.card_border;

        let mut children: Vec<RenderNode> = Vec::new();

        draw_section_header(
            &mut children,
            w,
            "Layers (PushLayer / PopLayer)",
            card_border,
            muted,
        );

        children.push(RenderNode::Primitive(DrawCommand::Text(Arc::new(
            TextPayload {
                text: crate::static_rc_str!("Opacity — same red rect at 1.0 / 0.6 / 0.3 / 0.1"),
                rect: Rect {
                    x: 0.0,
                    y: 40.0,
                    width: 500.0,
                    height: 16.0,
                },
                style: TextStyle::new(11.0, muted),
            },
        ))));

        for (i, &opacity) in [1.0f32, 0.6, 0.3, 0.1].iter().enumerate() {
            let x = i as f32 * 184.0;
            children.push(RenderNode::Layer {
                opacity,
                backdrop_blur: 0.0,
                children: NodeVec::collect([
                    RenderNode::Primitive(DrawCommand::Rect(Arc::new(RectPayload {
                        rect: Rect {
                            x,
                            y: 60.0,
                            width: 168.0,
                            height: 80.0,
                        },
                        style: RectStyle {
                            fill: Some(Paint::Solid(danger)),
                            stroke: None,
                            shadow: None,
                            radius: BorderRadius::all(8.0),
                        },
                    }))),
                    RenderNode::Primitive(DrawCommand::Text(Arc::new(TextPayload {
                        text: Arc::from(format!("{opacity:.1}")),
                        rect: Rect {
                            x,
                            y: 64.0,
                            width: 168.0,
                            height: 72.0,
                        },
                        style: TextStyle::new(18.0, Color::WHITE),
                    }))),
                ]),
            });
        }

        children.push(RenderNode::Primitive(DrawCommand::Text(Arc::new(
            TextPayload {
                text: crate::static_rc_str!("Overlapping colored layers at 0.7 opacity"),
                rect: Rect {
                    x: 0.0,
                    y: 164.0,
                    width: 500.0,
                    height: 16.0,
                },
                style: TextStyle::new(11.0, muted),
            },
        ))));

        children.push(RenderNode::Primitive(DrawCommand::Rect(Arc::new(
            RectPayload {
                rect: Rect {
                    x: 0.0,
                    y: 184.0,
                    width: 368.0,
                    height: 180.0,
                },
                style: RectStyle {
                    fill: Some(Paint::Solid(dark)),
                    stroke: None,
                    shadow: None,
                    radius: BorderRadius::all(8.0),
                },
            },
        ))));

        children.push(RenderNode::Layer {
            opacity: 0.7,
            backdrop_blur: 0.0,
            children: NodeVec::collect([RenderNode::Primitive(DrawCommand::Rect(Arc::new(
                RectPayload {
                    rect: Rect {
                        x: 16.0,
                        y: 200.0,
                        width: 180.0,
                        height: 120.0,
                    },
                    style: RectStyle {
                        fill: Some(Paint::Solid(primary)),
                        stroke: None,
                        shadow: None,
                        radius: BorderRadius::all(8.0),
                    },
                },
            )))]),
        });

        children.push(RenderNode::Layer {
            opacity: 0.7,
            backdrop_blur: 0.0,
            children: NodeVec::collect([RenderNode::Primitive(DrawCommand::Rect(Arc::new(
                RectPayload {
                    rect: Rect {
                        x: 96.0,
                        y: 240.0,
                        width: 180.0,
                        height: 120.0,
                    },
                    style: RectStyle {
                        fill: Some(Paint::Solid(success)),
                        stroke: None,
                        shadow: None,
                        radius: BorderRadius::all(8.0),
                    },
                },
            )))]),
        });

        children.push(RenderNode::Layer {
            opacity: 0.7,
            backdrop_blur: 0.0,
            children: NodeVec::collect([RenderNode::Primitive(DrawCommand::Rect(Arc::new(
                RectPayload {
                    rect: Rect {
                        x: 176.0,
                        y: 220.0,
                        width: 180.0,
                        height: 120.0,
                    },
                    style: RectStyle {
                        fill: Some(Paint::Solid(danger)),
                        stroke: None,
                        shadow: None,
                        radius: BorderRadius::all(8.0),
                    },
                },
            )))]),
        });

        children.push(RenderNode::Primitive(DrawCommand::Text(Arc::new(
            TextPayload {
                text: crate::static_rc_str!("Layer (0.8) wrapping a gradient rect + text"),
                rect: Rect {
                    x: 396.0,
                    y: 164.0,
                    width: 360.0,
                    height: 16.0,
                },
                style: TextStyle::new(11.0, muted),
            },
        ))));

        children.push(RenderNode::Layer {
            opacity: 0.8,
            backdrop_blur: 0.0,
            children: NodeVec::collect([
                RenderNode::Primitive(DrawCommand::Rect(Arc::new(RectPayload {
                    rect: Rect {
                        x: 396.0,
                        y: 184.0,
                        width: 320.0,
                        height: 180.0,
                    },
                    style: RectStyle {
                        fill: Some(Paint::Gradient(Gradient::linear(
                            Point::new(396.0, 274.0),
                            Point::new(716.0, 274.0),
                            &[(0.0, primary), (0.5, purple), (1.0, danger)],
                        ))),
                        stroke: None,
                        shadow: None,
                        radius: BorderRadius::all(12.0),
                    },
                }))),
                RenderNode::Primitive(DrawCommand::Text(Arc::new(TextPayload {
                    text: crate::static_rc_str!("gradient + layer"),
                    rect: Rect {
                        x: 396.0,
                        y: 254.0,
                        width: 320.0,
                        height: 60.0,
                    },
                    style: TextStyle::new(18.0, Color::WHITE),
                }))),
            ]),
        });

        children.push(RenderNode::Primitive(DrawCommand::Text(Arc::new(
            TextPayload {
                text: crate::static_rc_str!("Nested layers: outer 0.6, inner 0.5 → combined ~0.3"),
                rect: Rect {
                    x: 0.0,
                    y: 390.0,
                    width: 500.0,
                    height: 16.0,
                },
                style: TextStyle::new(11.0, muted),
            },
        ))));

        children.push(RenderNode::Layer {
            opacity: 0.6,
            backdrop_blur: 0.0,
            children: NodeVec::collect([
                RenderNode::Primitive(DrawCommand::Rect(Arc::new(RectPayload {
                    rect: Rect {
                        x: 0.0,
                        y: 410.0,
                        width: 340.0,
                        height: 120.0,
                    },
                    style: RectStyle {
                        fill: Some(Paint::Solid(primary)),
                        stroke: None,
                        shadow: None,
                        radius: BorderRadius::all(8.0),
                    },
                }))),
                RenderNode::Layer {
                    opacity: 0.5,
                    backdrop_blur: 0.0,
                    children: NodeVec::collect([
                        RenderNode::Primitive(DrawCommand::Rect(Arc::new(RectPayload {
                            rect: Rect {
                                x: 36.0,
                                y: 430.0,
                                width: 260.0,
                                height: 80.0,
                            },
                            style: RectStyle {
                                fill: Some(Paint::Solid(danger)),
                                stroke: None,
                                shadow: None,
                                radius: BorderRadius::all(6.0),
                            },
                        }))),
                        RenderNode::Primitive(DrawCommand::Text(Arc::new(TextPayload {
                            text: crate::static_rc_str!("inner 0.5"),
                            rect: Rect {
                                x: 36.0,
                                y: 434.0,
                                width: 260.0,
                                height: 72.0,
                            },
                            style: TextStyle::new(14.0, Color::WHITE),
                        }))),
                    ]),
                },
                RenderNode::Primitive(DrawCommand::Text(Arc::new(TextPayload {
                    text: crate::static_rc_str!("outer 0.6"),
                    rect: Rect {
                        x: 0.0,
                        y: 414.0,
                        width: 340.0,
                        height: 20.0,
                    },
                    style: TextStyle::new(11.0, Color::rgba(1.0, 1.0, 1.0, 0.7)),
                }))),
            ]),
        });

        RenderNode::group(children)
    })
}
