use std::rc::Rc;

use rsx::{
    BorderRadius, Color, Component, DrawCommand, DrawingArea, LayoutError, LayoutStyle, Line,
    LineStyle, Paint, Point, Rect, RectPayload, RectStyle, RenderNode, Shadow, Stroke, TextPayload,
    TextStyle, WidgetCtx, use_theme,
};

use crate::theme::SandboxTheme;

pub fn shadows_section(ctx: &mut WidgetCtx) -> Result<DrawingArea, LayoutError> {
    DrawingArea::new(ctx, LayoutStyle::new().height(640.0), |w, _h| {
        let t = use_theme::<SandboxTheme>();
        let primary = t.primary;
        let success = t.success;
        let danger = t.danger;
        let purple = t.purple;
        let dark = t.dark;
        let muted = t.muted;
        let card_border = t.card_border;

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
                text: Rc::from("Shadows"),
                rect: Rect {
                    x: 0.0,
                    y: 12.0,
                    width: 300.0,
                    height: 20.0,
                },
                style: TextStyle::new(12.0, muted),
            },
        ))));

        children.push(RenderNode::Primitive(DrawCommand::Text(Box::new(
            TextPayload {
                text: Rc::from("Rect shadows — offset / blur / spread"),
                rect: Rect {
                    x: 0.0,
                    y: 40.0,
                    width: 400.0,
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
                    width: 152.0,
                    height: 80.0,
                },
                style: RectStyle {
                    fill: Some(Paint::Solid(Color::WHITE)),
                    stroke: None,
                    shadow: Some(Shadow::new(
                        0.0,
                        4.0,
                        12.0,
                        Color::rgba(0.0, 0.0, 0.0, 0.25),
                    )),
                    radius: BorderRadius::all(8.0),
                },
            },
        ))));
        children.push(RenderNode::Primitive(DrawCommand::Text(Box::new(
            TextPayload {
                text: Rc::from("soft (0, 4, 12)"),
                rect: Rect {
                    x: 0.0,
                    y: 146.0,
                    width: 152.0,
                    height: 16.0,
                },
                style: TextStyle::new(11.0, muted),
            },
        ))));

        children.push(RenderNode::Primitive(DrawCommand::Rect(Box::new(
            RectPayload {
                rect: Rect {
                    x: 176.0,
                    y: 60.0,
                    width: 152.0,
                    height: 80.0,
                },
                style: RectStyle {
                    fill: Some(Paint::Solid(Color::WHITE)),
                    stroke: None,
                    shadow: Some(Shadow::new(4.0, 8.0, 4.0, Color::rgba(0.0, 0.0, 0.0, 0.4))),
                    radius: BorderRadius::all(8.0),
                },
            },
        ))));
        children.push(RenderNode::Primitive(DrawCommand::Text(Box::new(
            TextPayload {
                text: Rc::from("offset (4, 8, 4)"),
                rect: Rect {
                    x: 176.0,
                    y: 146.0,
                    width: 152.0,
                    height: 16.0,
                },
                style: TextStyle::new(11.0, muted),
            },
        ))));

        children.push(RenderNode::Primitive(DrawCommand::Rect(Box::new(
            RectPayload {
                rect: Rect {
                    x: 352.0,
                    y: 60.0,
                    width: 152.0,
                    height: 80.0,
                },
                style: RectStyle {
                    fill: Some(Paint::Solid(Color::WHITE)),
                    stroke: None,
                    shadow: Some(Shadow::new(0.0, 6.0, 16.0, primary.with_alpha(0.5))),
                    radius: BorderRadius::all(8.0),
                },
            },
        ))));
        children.push(RenderNode::Primitive(DrawCommand::Text(Box::new(
            TextPayload {
                text: Rc::from("colored primary"),
                rect: Rect {
                    x: 352.0,
                    y: 146.0,
                    width: 152.0,
                    height: 16.0,
                },
                style: TextStyle::new(11.0, muted),
            },
        ))));

        children.push(RenderNode::Primitive(DrawCommand::Rect(Box::new(
            RectPayload {
                rect: Rect {
                    x: 528.0,
                    y: 60.0,
                    width: 152.0,
                    height: 80.0,
                },
                style: RectStyle {
                    fill: Some(Paint::Solid(Color::WHITE)),
                    stroke: None,
                    shadow: Some(
                        Shadow::new(0.0, 0.0, 8.0, Color::rgba(0.0, 0.0, 0.0, 0.3))
                            .with_spread(4.0),
                    ),
                    radius: BorderRadius::all(8.0),
                },
            },
        ))));
        children.push(RenderNode::Primitive(DrawCommand::Text(Box::new(
            TextPayload {
                text: Rc::from("spread +4"),
                rect: Rect {
                    x: 528.0,
                    y: 146.0,
                    width: 152.0,
                    height: 16.0,
                },
                style: TextStyle::new(11.0, muted),
            },
        ))));

        children.push(RenderNode::Primitive(DrawCommand::Text(Box::new(
            TextPayload {
                text: Rc::from("Colored shadows on dark cards"),
                rect: Rect {
                    x: 0.0,
                    y: 176.0,
                    width: 400.0,
                    height: 16.0,
                },
                style: TextStyle::new(11.0, muted),
            },
        ))));

        let card_colors: &[(Color, Color, &str)] = &[
            (primary, primary.with_alpha(0.6), "primary glow"),
            (success, success.with_alpha(0.6), "success glow"),
            (danger, danger.with_alpha(0.6), "danger glow"),
            (purple, purple.with_alpha(0.6), "purple glow"),
        ];
        for (i, &(card_color, shadow_color, label)) in card_colors.iter().enumerate() {
            let x = i as f32 * 184.0;
            children.push(RenderNode::Primitive(DrawCommand::Rect(Box::new(
                RectPayload {
                    rect: Rect {
                        x,
                        y: 196.0,
                        width: 168.0,
                        height: 80.0,
                    },
                    style: RectStyle {
                        fill: Some(Paint::Solid(card_color)),
                        stroke: None,
                        shadow: Some(Shadow::new(0.0, 8.0, 20.0, shadow_color)),
                        radius: BorderRadius::all(10.0),
                    },
                },
            ))));
            children.push(RenderNode::Primitive(DrawCommand::Text(Box::new(
                TextPayload {
                    text: Rc::from(label),
                    rect: Rect {
                        x,
                        y: 200.0,
                        width: 168.0,
                        height: 72.0,
                    },
                    style: TextStyle::new(12.0, Color::WHITE),
                },
            ))));
        }

        children.push(RenderNode::Primitive(DrawCommand::Text(Box::new(
            TextPayload {
                text: Rc::from("Text shadows"),
                rect: Rect {
                    x: 0.0,
                    y: 312.0,
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
                    y: 332.0,
                    width: 720.0,
                    height: 100.0,
                },
                style: RectStyle {
                    fill: Some(Paint::Solid(Color::WHITE)),
                    stroke: Some(Stroke::new(card_border, 1.0)),
                    shadow: None,
                    radius: BorderRadius::all(8.0),
                },
            },
        ))));

        children.push(RenderNode::Primitive(DrawCommand::Text(Box::new(
            TextPayload {
                text: Rc::from("Drop shadow"),
                rect: Rect {
                    x: 16.0,
                    y: 348.0,
                    width: 180.0,
                    height: 30.0,
                },
                style: TextStyle::new(22.0, dark).with_shadow(Shadow::new(
                    2.0,
                    3.0,
                    5.0,
                    Color::rgba(0.0, 0.0, 0.0, 0.35),
                )),
            },
        ))));

        children.push(RenderNode::Primitive(DrawCommand::Text(Box::new(
            TextPayload {
                text: Rc::from("Color glow"),
                rect: Rect {
                    x: 216.0,
                    y: 348.0,
                    width: 200.0,
                    height: 30.0,
                },
                style: TextStyle::new(22.0, primary).with_shadow(Shadow::new(
                    0.0,
                    0.0,
                    8.0,
                    primary.with_alpha(0.6),
                )),
            },
        ))));

        children.push(RenderNode::Primitive(DrawCommand::Text(Box::new(
            TextPayload {
                text: Rc::from("Hard offset"),
                rect: Rect {
                    x: 436.0,
                    y: 348.0,
                    width: 200.0,
                    height: 30.0,
                },
                style: TextStyle::new(22.0, dark).with_shadow(Shadow::new(
                    3.0,
                    3.0,
                    1.0,
                    danger.with_alpha(0.7),
                )),
            },
        ))));

        children.push(RenderNode::Primitive(DrawCommand::Text(Box::new(
            TextPayload {
                text: Rc::from("Shadow inside layer"),
                rect: Rect {
                    x: 0.0,
                    y: 452.0,
                    width: 400.0,
                    height: 16.0,
                },
                style: TextStyle::new(11.0, muted),
            },
        ))));

        children.push(RenderNode::Layer {
            opacity: 1.0,
            children: vec![
                RenderNode::Primitive(DrawCommand::Rect(Box::new(RectPayload {
                    rect: Rect {
                        x: 0.0,
                        y: 472.0,
                        width: 220.0,
                        height: 100.0,
                    },
                    style: RectStyle {
                        fill: Some(Paint::Solid(Color::WHITE)),
                        stroke: None,
                        shadow: Some(Shadow::new(0.0, 6.0, 16.0, Color::rgba(0.0, 0.0, 0.0, 0.2))),
                        radius: BorderRadius::all(10.0),
                    },
                }))),
                RenderNode::Primitive(DrawCommand::Text(Box::new(TextPayload {
                    text: Rc::from("layer opacity 1.0"),
                    rect: Rect {
                        x: 16.0,
                        y: 500.0,
                        width: 200.0,
                        height: 20.0,
                    },
                    style: TextStyle::new(12.0, dark),
                }))),
            ],
        });

        children.push(RenderNode::Layer {
            opacity: 0.7,
            children: vec![
                RenderNode::Primitive(DrawCommand::Rect(Box::new(RectPayload {
                    rect: Rect {
                        x: 240.0,
                        y: 472.0,
                        width: 220.0,
                        height: 100.0,
                    },
                    style: RectStyle {
                        fill: Some(Paint::Solid(Color::WHITE)),
                        stroke: None,
                        shadow: Some(Shadow::new(0.0, 6.0, 16.0, primary.with_alpha(0.5))),
                        radius: BorderRadius::all(10.0),
                    },
                }))),
                RenderNode::Primitive(DrawCommand::Text(Box::new(TextPayload {
                    text: Rc::from("layer opacity 0.7"),
                    rect: Rect {
                        x: 256.0,
                        y: 500.0,
                        width: 200.0,
                        height: 20.0,
                    },
                    style: TextStyle::new(12.0, dark),
                }))),
            ],
        });

        children.push(RenderNode::Layer {
            opacity: 0.4,
            children: vec![
                RenderNode::Primitive(DrawCommand::Rect(Box::new(RectPayload {
                    rect: Rect {
                        x: 480.0,
                        y: 472.0,
                        width: 220.0,
                        height: 100.0,
                    },
                    style: RectStyle {
                        fill: Some(Paint::Solid(Color::WHITE)),
                        stroke: None,
                        shadow: Some(Shadow::new(4.0, 4.0, 8.0, purple.with_alpha(0.6))),
                        radius: BorderRadius::all(10.0),
                    },
                }))),
                RenderNode::Primitive(DrawCommand::Text(Box::new(TextPayload {
                    text: Rc::from("layer opacity 0.4"),
                    rect: Rect {
                        x: 496.0,
                        y: 500.0,
                        width: 200.0,
                        height: 20.0,
                    },
                    style: TextStyle::new(12.0, dark),
                }))),
            ],
        });

        RenderNode::group(children)
    })
}
