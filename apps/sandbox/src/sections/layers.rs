use std::rc::Rc;

use rsx::{
    BorderRadius, Color, Component, DrawCommand, DrawingArea, LayoutError, LayoutStyle, Line,
    LineStyle, LinearGradient, Paint, Point, Rect, RectPayload, RectStyle, TextPayload, TextStyle,
    View, WidgetCtx, use_theme,
};

use crate::theme::SandboxTheme;

pub fn layers_section(ctx: &mut WidgetCtx) -> Result<DrawingArea, LayoutError> {
    DrawingArea::new(ctx, LayoutStyle::new().height(560.0), |w, _h| {
        let t = use_theme::<SandboxTheme>();
        let primary = t.primary;
        let success = t.success;
        let danger = t.danger;
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
            text: Rc::from("Layers (PushLayer / PopLayer)"),
            rect: Rect {
                x: 0.0,
                y: 12.0,
                width: 400.0,
                height: 20.0,
            },
            style: TextStyle::new(12.0, muted),
        }))));

        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("Opacity — same red rect at 1.0 / 0.6 / 0.3 / 0.1"),
            rect: Rect {
                x: 0.0,
                y: 40.0,
                width: 500.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, muted),
        }))));

        for (i, &opacity) in [1.0f32, 0.6, 0.3, 0.1].iter().enumerate() {
            let x = i as f32 * 184.0;
            children.push(View::Layer {
                opacity,
                children: vec![
                    View::Primitive(DrawCommand::Rect(Box::new(RectPayload {
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
                    View::Primitive(DrawCommand::Text(Box::new(TextPayload {
                        text: Rc::from(format!("{opacity:.1}")),
                        rect: Rect {
                            x,
                            y: 64.0,
                            width: 168.0,
                            height: 72.0,
                        },
                        style: TextStyle::new(18.0, Color::WHITE),
                    }))),
                ],
            });
        }

        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("Overlapping colored layers at 0.7 opacity"),
            rect: Rect {
                x: 0.0,
                y: 164.0,
                width: 500.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, muted),
        }))));

        children.push(View::Primitive(DrawCommand::Rect(Box::new(RectPayload {
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
        }))));

        children.push(View::Layer {
            opacity: 0.7,
            children: vec![View::Primitive(DrawCommand::Rect(Box::new(RectPayload {
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
            })))],
        });

        children.push(View::Layer {
            opacity: 0.7,
            children: vec![View::Primitive(DrawCommand::Rect(Box::new(RectPayload {
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
            })))],
        });

        children.push(View::Layer {
            opacity: 0.7,
            children: vec![View::Primitive(DrawCommand::Rect(Box::new(RectPayload {
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
            })))],
        });

        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("Layer (0.8) wrapping a gradient rect + text"),
            rect: Rect {
                x: 396.0,
                y: 164.0,
                width: 360.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, muted),
        }))));

        children.push(View::Layer {
            opacity: 0.8,
            children: vec![
                View::Primitive(DrawCommand::Rect(Box::new(RectPayload {
                    rect: Rect {
                        x: 396.0,
                        y: 184.0,
                        width: 320.0,
                        height: 180.0,
                    },
                    style: RectStyle {
                        fill: Some(Paint::LinearGradient(LinearGradient::new(
                            Point::new(396.0, 274.0),
                            Point::new(716.0, 274.0),
                            &[(0.0, primary), (0.5, purple), (1.0, danger)],
                        ))),
                        stroke: None,
                        shadow: None,
                        radius: BorderRadius::all(12.0),
                    },
                }))),
                View::Primitive(DrawCommand::Text(Box::new(TextPayload {
                    text: Rc::from("gradient + layer"),
                    rect: Rect {
                        x: 396.0,
                        y: 254.0,
                        width: 320.0,
                        height: 60.0,
                    },
                    style: TextStyle::new(18.0, Color::WHITE),
                }))),
            ],
        });

        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("Nested layers: outer 0.6, inner 0.5 → combined ~0.3"),
            rect: Rect {
                x: 0.0,
                y: 390.0,
                width: 500.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, muted),
        }))));

        children.push(View::Layer {
            opacity: 0.6,
            children: vec![
                View::Primitive(DrawCommand::Rect(Box::new(RectPayload {
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
                View::Layer {
                    opacity: 0.5,
                    children: vec![
                        View::Primitive(DrawCommand::Rect(Box::new(RectPayload {
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
                        View::Primitive(DrawCommand::Text(Box::new(TextPayload {
                            text: Rc::from("inner 0.5"),
                            rect: Rect {
                                x: 36.0,
                                y: 434.0,
                                width: 260.0,
                                height: 72.0,
                            },
                            style: TextStyle::new(14.0, Color::WHITE),
                        }))),
                    ],
                },
                View::Primitive(DrawCommand::Text(Box::new(TextPayload {
                    text: Rc::from("outer 0.6"),
                    rect: Rect {
                        x: 0.0,
                        y: 414.0,
                        width: 340.0,
                        height: 20.0,
                    },
                    style: TextStyle::new(11.0, Color::rgba(1.0, 1.0, 1.0, 0.7)),
                }))),
            ],
        });

        View::group(children)
    })
}
