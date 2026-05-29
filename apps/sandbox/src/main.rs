use std::rc::Rc;

use rsx::{
    App, AvailableSpace, BorderRadius, Bounds, Color, Component, Container, DrawCommand,
    DrawingArea, Event, EventResult, FillRule, FillStyle, Image, ImageData, ImageFilter,
    LayoutError, LayoutItem, LayoutStyle, Line, LineCap, LineJoin, LineStyle, LinearGradient,
    NodeId, Path, PathData, PathStyle, Point, RadialGradient, RectPayload, RectStyle, RwSignal,
    ScrollArea, Shadow, Stroke, Text, TextPayload, TextStyle, Track, View, WidgetCtx, WindowConfig,
    compute_layout, create_rw_signal, with_context,
};

const SURFACE: Color = Color::rgba(0.95, 0.95, 0.97, 1.0);
const PRIMARY: Color = Color::rgba(0.24, 0.47, 0.98, 1.0);
const SUCCESS: Color = Color::rgba(0.18, 0.69, 0.45, 1.0);
const DANGER: Color = Color::rgba(0.92, 0.27, 0.27, 1.0);
const WARNING: Color = Color::rgba(0.97, 0.72, 0.18, 1.0);
const PURPLE: Color = Color::rgba(0.60, 0.28, 0.98, 1.0);
const DARK: Color = Color::rgba(0.08, 0.08, 0.14, 1.0);
const MUTED: Color = Color::rgba(0.50, 0.50, 0.60, 1.0);
const WHITE: Color = Color::rgba(1.0, 1.0, 1.0, 1.0);
const CARD_BORDER: Color = Color::rgba(0.80, 0.80, 0.88, 1.0);

fn heading(ctx: &mut WidgetCtx, label: &'static str) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let text = Text::new(
        ctx,
        move || label.to_string(),
        LayoutStyle::new().height(20.0),
        || TextStyle::new(12.0, MUTED),
    )?;
    Ok(Box::new(text) as Box<dyn LayoutItem>)
}

fn build_content(
    ctx: &mut WidgetCtx,
    gradient: Rc<ImageData>,
    checker: Rc<ImageData>,
    alpha: Rc<ImageData>,
) -> Result<Container, LayoutError> {
    let s1 = Box::new(shapes_section(ctx)?) as Box<dyn LayoutItem>;
    let s2 = Box::new(colors_section(ctx)?) as Box<dyn LayoutItem>;
    let s3 = Box::new(typography_section(ctx)?) as Box<dyn LayoutItem>;
    let s4 = Box::new(cards_section(ctx)?) as Box<dyn LayoutItem>;
    let s5 = Box::new(images_section(ctx, gradient, checker, alpha)?) as Box<dyn LayoutItem>;
    let s6 = Box::new(lines_section(ctx)?) as Box<dyn LayoutItem>;
    let s7 = Box::new(paths_section(ctx)?) as Box<dyn LayoutItem>;
    let s8 = Box::new(gradients_section(ctx)?) as Box<dyn LayoutItem>;
    let s9 = Box::new(layers_section(ctx)?) as Box<dyn LayoutItem>;
    let s10 = Box::new(shadows_section(ctx)?) as Box<dyn LayoutItem>;
    let s11 = Box::new(grid_section(ctx)?) as Box<dyn LayoutItem>;
    let sections = vec![s1, s2, s3, s4, s5, s6, s7, s8, s9, s10, s11];

    Container::new(
        ctx,
        LayoutStyle::new().flex_column().padding_all(24.0).gap(24.0),
        sections,
    )
}

fn shape_card(
    ctx: &mut WidgetCtx,
    style: RectStyle,
    label: &'static str,
    label_color: Color,
) -> Result<DrawingArea, LayoutError> {
    DrawingArea::new(
        ctx,
        LayoutStyle::new().width(168.0).height(80.0),
        move |_w, _h| {
            View::group([
                View::Primitive(DrawCommand::Rect(Box::new(RectPayload {
                    rect: Bounds {
                        x: 0.0,
                        y: 0.0,
                        width: 168.0,
                        height: 80.0,
                    },
                    style: style.clone(),
                }))),
                View::Primitive(DrawCommand::Text(Box::new(TextPayload {
                    text: Rc::from(label),
                    rect: Bounds {
                        x: 0.0,
                        y: 4.0,
                        width: 168.0,
                        height: 72.0,
                    },
                    style: TextStyle::new(13.0, label_color),
                }))),
            ])
        },
    )
}

fn shapes_section(ctx: &mut WidgetCtx) -> Result<Container, LayoutError> {
    let sc1 = Box::new(shape_card(
        ctx,
        RectStyle {
            fill: Some(FillStyle::Solid(PRIMARY)),
            stroke: None,
            radius: BorderRadius::all(8.0),
            shadow: None,
        },
        "fill",
        WHITE,
    )?) as Box<dyn LayoutItem>;
    let sc2 = Box::new(shape_card(
        ctx,
        RectStyle {
            fill: None,
            stroke: Some(Stroke::new(DANGER, 2.0)),
            radius: BorderRadius::all(8.0),
            shadow: None,
        },
        "stroke",
        DANGER,
    )?) as Box<dyn LayoutItem>;
    let sc3 = Box::new(shape_card(
        ctx,
        RectStyle {
            fill: Some(FillStyle::Solid(SUCCESS)),
            stroke: Some(Stroke::new(DARK, 1.5)),
            radius: BorderRadius::zero(),
            shadow: None,
        },
        "fill + stroke",
        WHITE,
    )?) as Box<dyn LayoutItem>;
    let sc4 = Box::new(shape_card(
        ctx,
        RectStyle {
            fill: Some(FillStyle::Solid(PURPLE)),
            stroke: None,
            radius: BorderRadius::all(40.0),
            shadow: None,
        },
        "pill radius",
        WHITE,
    )?) as Box<dyn LayoutItem>;
    let cards = Container::new(
        ctx,
        LayoutStyle::new().flex_row().gap(16.0),
        vec![sc1, sc2, sc3, sc4],
    )?;
    let h = heading(ctx, "Shapes")?;
    Container::new(
        ctx,
        LayoutStyle::new().flex_column().gap(8.0),
        vec![h, Box::new(cards) as Box<dyn LayoutItem>],
    )
}

fn color_swatch(
    ctx: &mut WidgetCtx,
    color: Color,
    label: &'static str,
) -> Result<DrawingArea, LayoutError> {
    DrawingArea::new(
        ctx,
        LayoutStyle::new().width(100.0).height(44.0),
        move |_w, _h| {
            View::group([
                View::Primitive(DrawCommand::Rect(Box::new(RectPayload {
                    rect: Bounds {
                        x: 0.0,
                        y: 0.0,
                        width: 100.0,
                        height: 44.0,
                    },
                    style: RectStyle {
                        fill: Some(FillStyle::Solid(color)),
                        stroke: None,
                        radius: BorderRadius::all(6.0),
                        shadow: None,
                    },
                }))),
                View::Primitive(DrawCommand::Text(Box::new(TextPayload {
                    text: Rc::from(label),
                    rect: Bounds {
                        x: 0.0,
                        y: 4.0,
                        width: 100.0,
                        height: 36.0,
                    },
                    style: TextStyle::new(11.0, WHITE),
                }))),
            ])
        },
    )
}

fn colors_section(ctx: &mut WidgetCtx) -> Result<Container, LayoutError> {
    let swatches = [PRIMARY, SUCCESS, DANGER, WARNING, PURPLE, DARK];
    let labels = ["primary", "success", "danger", "warning", "purple", "dark"];

    let mut row_children: Vec<Box<dyn LayoutItem>> = Vec::new();
    for (&color, &label) in swatches.iter().zip(labels.iter()) {
        row_children.push(Box::new(color_swatch(ctx, color, label)?) as Box<dyn LayoutItem>);
    }

    let row = Container::new(ctx, LayoutStyle::new().flex_row().gap(16.0), row_children)?;
    let h = heading(ctx, "Colors")?;
    Container::new(
        ctx,
        LayoutStyle::new().flex_column().gap(8.0),
        vec![h, Box::new(row) as Box<dyn LayoutItem>],
    )
}

fn type_line(
    ctx: &mut WidgetCtx,
    label: &'static str,
    size: f32,
    color: Color,
    height: f32,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let text = Text::new(
        ctx,
        move || label.to_string(),
        LayoutStyle::new().height(height),
        move || TextStyle::new(size, color),
    )?;
    Ok(Box::new(text) as Box<dyn LayoutItem>)
}

fn typography_section(ctx: &mut WidgetCtx) -> Result<Container, LayoutError> {
    let h = heading(ctx, "Typography")?;
    let t1 = type_line(ctx, "Small — 12px — The quick brown fox", 12.0, DARK, 20.0)?;
    let t2 = type_line(
        ctx,
        "Regular — 14px — The quick brown fox",
        14.0,
        DARK,
        22.0,
    )?;
    let t3 = type_line(ctx, "Medium — 18px — The quick brown fox", 18.0, DARK, 26.0)?;
    let t4 = type_line(ctx, "Large — 24px — The quick brown fox", 24.0, DARK, 32.0)?;
    let t5 = type_line(ctx, "Display — 32px", 32.0, PRIMARY, 42.0)?;
    Container::new(
        ctx,
        LayoutStyle::new().flex_column().gap(8.0),
        vec![h, t1, t2, t3, t4, t5],
    )
}

fn info_card(
    ctx: &mut WidgetCtx,
    bg: RectStyle,
    title: &'static str,
    title_color: Color,
    body: &'static str,
) -> Result<DrawingArea, LayoutError> {
    DrawingArea::new(
        ctx,
        LayoutStyle::new().width(368.0).height(110.0),
        move |_w, _h| {
            View::group([
                View::Primitive(DrawCommand::Rect(Box::new(RectPayload {
                    rect: Bounds {
                        x: 0.0,
                        y: 0.0,
                        width: 368.0,
                        height: 110.0,
                    },
                    style: bg.clone(),
                }))),
                View::Primitive(DrawCommand::Text(Box::new(TextPayload {
                    text: Rc::from(title),
                    rect: Bounds {
                        x: 16.0,
                        y: 14.0,
                        width: 340.0,
                        height: 24.0,
                    },
                    style: TextStyle::new(16.0, title_color),
                }))),
                View::Primitive(DrawCommand::Text(Box::new(TextPayload {
                    text: Rc::from(body),
                    rect: Bounds {
                        x: 16.0,
                        y: 44.0,
                        width: 340.0,
                        height: 52.0,
                    },
                    style: TextStyle::new(13.0, MUTED),
                }))),
            ])
        },
    )
}

fn cards_section(ctx: &mut WidgetCtx) -> Result<Container, LayoutError> {
    let c1 = Box::new(info_card(
        ctx,
        RectStyle {
            fill: Some(FillStyle::Solid(DARK)),
            stroke: None,
            radius: BorderRadius::all(10.0),
            shadow: None,
        },
        "Dark Card",
        WHITE,
        "White text on a dark background.",
    )?) as Box<dyn LayoutItem>;
    let c2 = Box::new(info_card(
        ctx,
        RectStyle {
            fill: Some(FillStyle::Solid(WHITE)),
            stroke: Some(Stroke::new(CARD_BORDER, 1.0)),
            radius: BorderRadius::all(10.0),
            shadow: None,
        },
        "Light Card",
        DARK,
        "Dark text on a white background.",
    )?) as Box<dyn LayoutItem>;
    let row = Container::new(ctx, LayoutStyle::new().flex_row().gap(16.0), vec![c1, c2])?;
    let h = heading(ctx, "Cards")?;
    Container::new(
        ctx,
        LayoutStyle::new().flex_column().gap(8.0),
        vec![h, Box::new(row) as Box<dyn LayoutItem>],
    )
}

fn image_with_label(
    ctx: &mut WidgetCtx,
    data: Rc<ImageData>,
    filter: ImageFilter,
    size: f32,
    label: &'static str,
) -> Result<Container, LayoutError> {
    let image = Image::new(
        ctx,
        {
            let d = data.clone();
            move || d.clone()
        },
        LayoutStyle::new().width(size).height(size),
        move || filter,
    )?;
    let label = Text::new(
        ctx,
        move || label.to_string(),
        LayoutStyle::new().width(size).height(16.0),
        || TextStyle::new(11.0, MUTED),
    )?;

    Container::new(
        ctx,
        LayoutStyle::new().flex_column().gap(4.0),
        vec![
            Box::new(image) as Box<dyn LayoutItem>,
            Box::new(label) as Box<dyn LayoutItem>,
        ],
    )
}

fn images_section(
    ctx: &mut WidgetCtx,
    gradient: Rc<ImageData>,
    checker: Rc<ImageData>,
    alpha: Rc<ImageData>,
) -> Result<Container, LayoutError> {
    let i1 = Box::new(image_with_label(
        ctx,
        gradient,
        ImageFilter::Linear,
        128.0,
        "gradient",
    )?) as Box<dyn LayoutItem>;
    let i2 = Box::new(image_with_label(
        ctx,
        checker,
        ImageFilter::Nearest,
        192.0,
        "checker (scaled)",
    )?) as Box<dyn LayoutItem>;
    let i3 = Box::new(image_with_label(
        ctx,
        alpha,
        ImageFilter::Nearest,
        128.0,
        "alpha blend",
    )?) as Box<dyn LayoutItem>;
    let row = Container::new(
        ctx,
        LayoutStyle::new().flex_row().gap(20.0),
        vec![i1, i2, i3],
    )?;
    let h = heading(ctx, "Images")?;
    Container::new(
        ctx,
        LayoutStyle::new().flex_column().gap(8.0),
        vec![h, Box::new(row) as Box<dyn LayoutItem>],
    )
}

fn lines_section(ctx: &mut WidgetCtx) -> Result<DrawingArea, LayoutError> {
    DrawingArea::new(ctx, LayoutStyle::new().height(330.0), |w, _h| {
        let mut children: Vec<View> = Vec::new();

        children.push(
            Line::new(
                || Point::new(0.0, 0.0),
                move || Point::new(w, 0.0),
                || LineStyle::new(CARD_BORDER, 1.0),
            )
            .view(),
        );
        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("Lines"),
            rect: Bounds {
                x: 0.0,
                y: 12.0,
                width: 200.0,
                height: 20.0,
            },
            style: TextStyle::new(12.0, MUTED),
        }))));

        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("Width"),
            rect: Bounds {
                x: 0.0,
                y: 40.0,
                width: 60.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, MUTED),
        }))));

        let width_examples: &[(f32, &str)] = &[
            (1.0, "1 px"),
            (2.0, "2 px"),
            (4.0, "4 px"),
            (8.0, "8 px"),
            (16.0, "16 px"),
        ];
        let mut cy = 62.0f32;
        for &(w, label) in width_examples {
            children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
                text: Rc::from(label),
                rect: Bounds {
                    x: 0.0,
                    y: cy - 8.0,
                    width: 56.0,
                    height: 16.0,
                },
                style: TextStyle::new(11.0, MUTED),
            }))));
            children.push(
                Line::new(
                    move || Point::new(64.0, cy),
                    move || Point::new(336.0, cy),
                    move || LineStyle::new(PRIMARY, w),
                )
                .view(),
            );
            cy += w.max(2.0) + 18.0;
        }

        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("Color"),
            rect: Bounds {
                x: 396.0,
                y: 40.0,
                width: 60.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, MUTED),
        }))));
        let color_examples: &[(Color, &str)] = &[
            (PRIMARY, "primary"),
            (SUCCESS, "success"),
            (DANGER, "danger"),
            (WARNING, "warning"),
            (PURPLE, "purple"),
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
            children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
                text: Rc::from(label),
                rect: Bounds {
                    x: 664.0,
                    y: y - 8.0,
                    width: 80.0,
                    height: 16.0,
                },
                style: TextStyle::new(11.0, color),
            }))));
        }

        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("Separator & chart"),
            rect: Bounds {
                x: 0.0,
                y: 176.0,
                width: 300.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, MUTED),
        }))));
        children.push(
            Line::new(
                || Point::new(0.0, 196.0),
                move || Point::new(w, 196.0),
                || LineStyle::new(CARD_BORDER, 1.0),
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
                || LineStyle::new(MUTED, 1.0),
            )
            .view(),
        );
        children.push(
            Line::new(
                move || Point::new(ax, cb),
                move || Point::new(ax_right, cb),
                || LineStyle::new(MUTED, 1.0),
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
                    || LineStyle::new(PRIMARY, 2.0),
                )
                .view(),
            );
            children.push(
                Line::new(
                    move || Point::new(data_x[i], s2[i]),
                    move || Point::new(data_x[i + 1], s2[i + 1]),
                    || LineStyle::new(SUCCESS, 2.0),
                )
                .view(),
            );
            children.push(
                Line::new(
                    move || Point::new(data_x[i], s3[i]),
                    move || Point::new(data_x[i + 1], s3[i + 1]),
                    || LineStyle::new(DANGER, 2.0),
                )
                .view(),
            );
        }

        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("Diagonals"),
            rect: Bounds {
                x: 436.0,
                y: 200.0,
                width: 120.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, MUTED),
        }))));
        let fan_cx = 566.0f32;
        let fan_cy = 266.0f32;
        let fan_tips: &[(f32, f32, Color)] = &[
            (476.0, 220.0, PRIMARY),
            (516.0, 214.0, SUCCESS),
            (566.0, 214.0, DANGER),
            (616.0, 214.0, WARNING),
            (656.0, 220.0, PURPLE),
            (686.0, 238.0, DARK),
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

        View::group(children)
    })
}

fn paths_section(ctx: &mut WidgetCtx) -> Result<DrawingArea, LayoutError> {
    DrawingArea::new(ctx, LayoutStyle::new().height(660.0), |w, _h| {
        let mut children: Vec<View> = Vec::new();

        children.push(
            Line::new(
                || Point::new(0.0, 0.0),
                move || Point::new(w, 0.0),
                || LineStyle::new(CARD_BORDER, 1.0),
            )
            .view(),
        );
        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("Paths"),
            rect: Bounds {
                x: 0.0,
                y: 12.0,
                width: 200.0,
                height: 20.0,
            },
            style: TextStyle::new(12.0, MUTED),
        }))));
        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("Polygon shapes"),
            rect: Bounds {
                x: 0.0,
                y: 36.0,
                width: 300.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, MUTED),
        }))));

        let triangle_data = Rc::new(
            PathData::new()
                .move_to(Point::new(75.0, 56.0))
                .line_to(Point::new(135.0, 166.0))
                .line_to(Point::new(15.0, 166.0))
                .close(),
        );
        children.push(
            Path::new(
                {
                    let d = triangle_data.clone();
                    move || d.clone()
                },
                || PathStyle {
                    fill: Some(FillStyle::Solid(PRIMARY)),
                    stroke: None,
                    fill_rule: FillRule::Winding,
                    shadow: None,
                },
            )
            .view(),
        );
        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("triangle"),
            rect: Bounds {
                x: 0.0,
                y: 176.0,
                width: 150.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, MUTED),
        }))));

        {
            let cx = 245.0f32;
            let cy = 111.0f32;
            let outer = 55.0f32;
            let inner = 22.0f32;
            let mut path = PathData::new();
            for i in 0..10usize {
                let angle = std::f32::consts::TAU * i as f32 / 10.0 - std::f32::consts::FRAC_PI_2;
                let r = if i % 2 == 0 { outer } else { inner };
                let p = Point::new(cx + r * angle.cos(), cy + r * angle.sin());
                path = if i == 0 {
                    path.move_to(p)
                } else {
                    path.line_to(p)
                };
            }
            path = path.close();
            let star_data = Rc::new(path);
            children.push(
                Path::new(
                    {
                        let d = star_data.clone();
                        move || d.clone()
                    },
                    || PathStyle {
                        fill: Some(FillStyle::Solid(DANGER)),
                        stroke: Some(Stroke::new(DARK, 1.0)),
                        fill_rule: FillRule::Winding,
                        shadow: None,
                    },
                )
                .view(),
            );
            children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
                text: Rc::from("star (fill + stroke)"),
                rect: Bounds {
                    x: 175.0,
                    y: 176.0,
                    width: 200.0,
                    height: 16.0,
                },
                style: TextStyle::new(11.0, MUTED),
            }))));
        }

        let evenodd_data = Rc::new(
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
            Path::new(
                {
                    let d = evenodd_data.clone();
                    move || d.clone()
                },
                || PathStyle {
                    fill: Some(FillStyle::Solid(PURPLE)),
                    stroke: None,
                    fill_rule: FillRule::EvenOdd,
                    shadow: None,
                },
            )
            .view(),
        );
        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("even-odd fill"),
            rect: Bounds {
                x: 350.0,
                y: 176.0,
                width: 200.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, MUTED),
        }))));

        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("Bézier curves"),
            rect: Bounds {
                x: 0.0,
                y: 212.0,
                width: 300.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, MUTED),
        }))));

        let quad_data = Rc::new(
            PathData::new()
                .move_to(Point::new(0.0, 308.0))
                .quad_to(Point::new(140.0, 238.0), Point::new(280.0, 308.0)),
        );
        children.push(
            Path::new(
                {
                    let d = quad_data.clone();
                    move || d.clone()
                },
                || PathStyle {
                    fill: None,
                    stroke: Some(Stroke::new(WARNING, 3.0).with_cap(LineCap::Round)),
                    fill_rule: FillRule::Winding,
                    shadow: None,
                },
            )
            .view(),
        );
        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("quad_to arch"),
            rect: Bounds {
                x: 0.0,
                y: 318.0,
                width: 200.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, MUTED),
        }))));

        let cubic_data = Rc::new(PathData::new().move_to(Point::new(310.0, 248.0)).cubic_to(
            Point::new(380.0, 248.0),
            Point::new(310.0, 308.0),
            Point::new(380.0, 308.0),
        ));
        children.push(
            Path::new(
                {
                    let d = cubic_data.clone();
                    move || d.clone()
                },
                || PathStyle {
                    fill: None,
                    stroke: Some(Stroke::new(SUCCESS, 3.0).with_cap(LineCap::Round)),
                    fill_rule: FillRule::Winding,
                    shadow: None,
                },
            )
            .view(),
        );
        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("cubic_to S-curve"),
            rect: Bounds {
                x: 296.0,
                y: 318.0,
                width: 200.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, MUTED),
        }))));

        let petal_data = Rc::new(
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
            Path::new(
                {
                    let d = petal_data.clone();
                    move || d.clone()
                },
                || PathStyle {
                    fill: Some(FillStyle::Solid(Color::rgba(0.97, 0.72, 0.18, 0.75))),
                    stroke: Some(Stroke::new(WARNING, 1.5)),
                    fill_rule: FillRule::Winding,
                    shadow: None,
                },
            )
            .view(),
        );
        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("closed cubic (petal)"),
            rect: Bounds {
                x: 446.0,
                y: 318.0,
                width: 200.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, MUTED),
        }))));

        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("Stroke style"),
            rect: Bounds {
                x: 0.0,
                y: 354.0,
                width: 300.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, MUTED),
        }))));

        let butt_data = Rc::new(
            PathData::new()
                .move_to(Point::new(0.0, 410.0))
                .line_to(Point::new(76.0, 390.0))
                .line_to(Point::new(152.0, 430.0))
                .line_to(Point::new(228.0, 390.0)),
        );
        children.push(
            Path::new(
                {
                    let d = butt_data.clone();
                    move || d.clone()
                },
                || PathStyle {
                    fill: None,
                    stroke: Some(Stroke::new(PRIMARY, 8.0)),
                    fill_rule: FillRule::Winding,
                    shadow: None,
                },
            )
            .view(),
        );
        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("Butt / Miter (default)"),
            rect: Bounds {
                x: 0.0,
                y: 448.0,
                width: 230.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, MUTED),
        }))));

        let round_data = Rc::new(
            PathData::new()
                .move_to(Point::new(300.0, 410.0))
                .line_to(Point::new(376.0, 390.0))
                .line_to(Point::new(452.0, 430.0))
                .line_to(Point::new(528.0, 390.0)),
        );
        children.push(
            Path::new(
                {
                    let d = round_data.clone();
                    move || d.clone()
                },
                || PathStyle {
                    fill: None,
                    stroke: Some(
                        Stroke::new(DANGER, 8.0)
                            .with_cap(LineCap::Round)
                            .with_join(LineJoin::Round),
                    ),
                    fill_rule: FillRule::Winding,
                    shadow: None,
                },
            )
            .view(),
        );
        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("Round cap / Round join"),
            rect: Bounds {
                x: 300.0,
                y: 448.0,
                width: 240.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, MUTED),
        }))));

        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("Path shadows"),
            rect: Bounds {
                x: 0.0,
                y: 490.0,
                width: 300.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, MUTED),
        }))));

        const K: f32 = 0.5523;
        let (cx1, cy1, r1) = (76.0_f32, 570.0_f32, 44.0_f32);
        let circle_data = Rc::new(
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
            Path::new(
                {
                    let d = circle_data.clone();
                    move || d.clone()
                },
                || {
                    PathStyle::default()
                        .with_fill(PRIMARY)
                        .with_shadow(Shadow::new(4.0, 6.0, 8.0, Color::rgba(0.0, 0.0, 0.0, 0.4)))
                },
            )
            .view(),
        );
        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("drop shadow"),
            rect: Bounds {
                x: 32.0,
                y: 624.0,
                width: 88.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, MUTED),
        }))));

        let star_shadow_data = Rc::new({
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
            Path::new(
                {
                    let d = star_shadow_data.clone();
                    move || d.clone()
                },
                || {
                    PathStyle::default()
                        .with_fill(WARNING)
                        .with_shadow(Shadow::new(
                            0.0,
                            0.0,
                            10.0,
                            Color::rgba(0.97, 0.72, 0.18, 0.7),
                        ))
                },
            )
            .view(),
        );
        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("glow"),
            rect: Bounds {
                x: 248.0,
                y: 624.0,
                width: 48.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, MUTED),
        }))));

        let (cx3, cy3, r3) = (468.0_f32, 570.0_f32, 44.0_f32);
        let diamond_data = Rc::new(
            PathData::new()
                .move_to(Point::new(cx3, cy3 - r3))
                .line_to(Point::new(cx3 + r3 * 0.65, cy3))
                .line_to(Point::new(cx3, cy3 + r3))
                .line_to(Point::new(cx3 - r3 * 0.65, cy3))
                .close(),
        );
        children.push(
            Path::new(
                {
                    let d = diamond_data.clone();
                    move || d.clone()
                },
                || {
                    PathStyle::default()
                        .with_fill(SUCCESS)
                        .with_shadow(Shadow::new(3.0, 3.0, 2.0, Color::rgba(0.0, 0.0, 0.0, 0.5)))
                },
            )
            .view(),
        );
        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("hard offset"),
            rect: Bounds {
                x: 428.0,
                y: 624.0,
                width: 80.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, MUTED),
        }))));

        let wave_data = Rc::new(
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
            Path::new(
                {
                    let d = wave_data.clone();
                    move || d.clone()
                },
                || {
                    PathStyle::default()
                        .with_stroke(Stroke::new(DANGER, 4.0).with_cap(LineCap::Round))
                        .with_shadow(Shadow::new(
                            2.0,
                            4.0,
                            6.0,
                            Color::rgba(0.92, 0.27, 0.27, 0.5),
                        ))
                },
            )
            .view(),
        );
        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("stroke shadow"),
            rect: Bounds {
                x: 600.0,
                y: 624.0,
                width: 100.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, MUTED),
        }))));

        View::group(children)
    })
}

fn gradients_section(ctx: &mut WidgetCtx) -> Result<DrawingArea, LayoutError> {
    DrawingArea::new(ctx, LayoutStyle::new().height(520.0), |w, _h| {
        let mut children: Vec<View> = Vec::new();

        children.push(
            Line::new(
                || Point::new(0.0, 0.0),
                move || Point::new(w, 0.0),
                || LineStyle::new(CARD_BORDER, 1.0),
            )
            .view(),
        );
        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("Gradients"),
            rect: Bounds {
                x: 0.0,
                y: 12.0,
                width: 200.0,
                height: 20.0,
            },
            style: TextStyle::new(12.0, MUTED),
        }))));

        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("Linear — Rect"),
            rect: Bounds {
                x: 0.0,
                y: 40.0,
                width: 300.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, MUTED),
        }))));

        children.push(View::Primitive(DrawCommand::Rect(Box::new(RectPayload {
            rect: Bounds {
                x: 0.0,
                y: 60.0,
                width: 168.0,
                height: 80.0,
            },
            style: RectStyle {
                fill: Some(FillStyle::LinearGradient(LinearGradient::new(
                    Point::new(0.0, 100.0),
                    Point::new(168.0, 100.0),
                    &[
                        (0.0, Color::rgba(0.92, 0.27, 0.27, 1.0)),
                        (1.0, Color::rgba(0.24, 0.47, 0.98, 1.0)),
                    ],
                ))),
                stroke: None,
                radius: BorderRadius::all(8.0),
                shadow: None,
            },
        }))));
        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("horizontal"),
            rect: Bounds {
                x: 0.0,
                y: 146.0,
                width: 168.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, MUTED),
        }))));

        children.push(View::Primitive(DrawCommand::Rect(Box::new(RectPayload {
            rect: Bounds {
                x: 184.0,
                y: 60.0,
                width: 168.0,
                height: 80.0,
            },
            style: RectStyle {
                fill: Some(FillStyle::LinearGradient(LinearGradient::new(
                    Point::new(268.0, 60.0),
                    Point::new(268.0, 140.0),
                    &[(0.0, PURPLE), (1.0, SUCCESS)],
                ))),
                stroke: None,
                radius: BorderRadius::all(8.0),
                shadow: None,
            },
        }))));
        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("vertical"),
            rect: Bounds {
                x: 184.0,
                y: 146.0,
                width: 168.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, MUTED),
        }))));

        children.push(View::Primitive(DrawCommand::Rect(Box::new(RectPayload {
            rect: Bounds {
                x: 368.0,
                y: 60.0,
                width: 168.0,
                height: 80.0,
            },
            style: RectStyle {
                fill: Some(FillStyle::LinearGradient(LinearGradient::new(
                    Point::new(368.0, 60.0),
                    Point::new(536.0, 140.0),
                    &[(0.0, WARNING), (1.0, DARK)],
                ))),
                stroke: None,
                radius: BorderRadius::all(8.0),
                shadow: None,
            },
        }))));
        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("diagonal"),
            rect: Bounds {
                x: 368.0,
                y: 146.0,
                width: 168.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, MUTED),
        }))));

        children.push(View::Primitive(DrawCommand::Rect(Box::new(RectPayload {
            rect: Bounds {
                x: 552.0,
                y: 60.0,
                width: 168.0,
                height: 80.0,
            },
            style: RectStyle {
                fill: Some(FillStyle::LinearGradient(LinearGradient::new(
                    Point::new(552.0, 100.0),
                    Point::new(720.0, 100.0),
                    &[
                        (0.0, DARK),
                        (0.5, Color::rgba(0.20, 0.75, 0.90, 1.0)),
                        (1.0, WHITE),
                    ],
                ))),
                stroke: None,
                radius: BorderRadius::all(8.0),
                shadow: None,
            },
        }))));
        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("3 stops"),
            rect: Bounds {
                x: 552.0,
                y: 146.0,
                width: 168.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, MUTED),
        }))));

        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("Radial — Rect"),
            rect: Bounds {
                x: 0.0,
                y: 180.0,
                width: 300.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, MUTED),
        }))));

        children.push(View::Primitive(DrawCommand::Rect(Box::new(RectPayload {
            rect: Bounds {
                x: 0.0,
                y: 200.0,
                width: 168.0,
                height: 80.0,
            },
            style: RectStyle {
                fill: Some(FillStyle::RadialGradient(RadialGradient::new(
                    Point::new(84.0, 240.0),
                    70.0,
                    &[(0.0, PRIMARY), (1.0, Color::rgba(0.24, 0.47, 0.98, 0.0))],
                ))),
                stroke: None,
                radius: BorderRadius::all(8.0),
                shadow: None,
            },
        }))));
        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("center burst"),
            rect: Bounds {
                x: 0.0,
                y: 286.0,
                width: 168.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, MUTED),
        }))));

        children.push(View::Primitive(DrawCommand::Rect(Box::new(RectPayload {
            rect: Bounds {
                x: 184.0,
                y: 200.0,
                width: 168.0,
                height: 80.0,
            },
            style: RectStyle {
                fill: Some(FillStyle::RadialGradient(RadialGradient::new(
                    Point::new(268.0, 240.0),
                    40.0,
                    &[(0.0, DANGER), (1.0, WARNING)],
                ))),
                stroke: None,
                radius: BorderRadius::all(8.0),
                shadow: None,
            },
        }))));
        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("tight radius"),
            rect: Bounds {
                x: 184.0,
                y: 286.0,
                width: 168.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, MUTED),
        }))));

        children.push(View::Primitive(DrawCommand::Rect(Box::new(RectPayload {
            rect: Bounds {
                x: 368.0,
                y: 200.0,
                width: 168.0,
                height: 80.0,
            },
            style: RectStyle {
                fill: Some(FillStyle::RadialGradient(RadialGradient::new(
                    Point::new(452.0, 240.0),
                    80.0,
                    &[(0.0, WHITE), (0.45, PURPLE), (1.0, DARK)],
                ))),
                stroke: None,
                radius: BorderRadius::all(8.0),
                shadow: None,
            },
        }))));
        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("3 stops"),
            rect: Bounds {
                x: 368.0,
                y: 286.0,
                width: 168.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, MUTED),
        }))));

        children.push(View::Primitive(DrawCommand::Rect(Box::new(RectPayload {
            rect: Bounds {
                x: 552.0,
                y: 200.0,
                width: 168.0,
                height: 80.0,
            },
            style: RectStyle {
                fill: Some(FillStyle::RadialGradient(RadialGradient::new(
                    Point::new(552.0, 200.0),
                    180.0,
                    &[(0.0, SUCCESS), (1.0, DARK)],
                ))),
                stroke: None,
                radius: BorderRadius::all(8.0),
                shadow: None,
            },
        }))));
        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("off-center"),
            rect: Bounds {
                x: 552.0,
                y: 286.0,
                width: 168.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, MUTED),
        }))));

        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("Gradients — Path"),
            rect: Bounds {
                x: 0.0,
                y: 318.0,
                width: 300.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, MUTED),
        }))));

        let tri = Rc::new(
            PathData::new()
                .move_to(Point::new(75.0, 338.0))
                .line_to(Point::new(150.0, 468.0))
                .line_to(Point::new(0.0, 468.0))
                .close(),
        );
        children.push(
            Path::new(
                {
                    let d = tri.clone();
                    move || d.clone()
                },
                || PathStyle {
                    fill: Some(FillStyle::LinearGradient(LinearGradient::new(
                        Point::new(75.0, 338.0),
                        Point::new(75.0, 468.0),
                        &[(0.0, DANGER), (1.0, WARNING)],
                    ))),
                    stroke: None,
                    fill_rule: FillRule::Winding,
                    shadow: None,
                },
            )
            .view(),
        );
        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("triangle linear"),
            rect: Bounds {
                x: 0.0,
                y: 476.0,
                width: 180.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, MUTED),
        }))));

        let cx = 268.0f32;
        let cy = 403.0f32;
        let outer = 65.0f32;
        let inner = 26.0f32;
        let mut star_path = PathData::new();
        for i in 0..10usize {
            let angle = std::f32::consts::TAU * i as f32 / 10.0 - std::f32::consts::FRAC_PI_2;
            let r = if i % 2 == 0 { outer } else { inner };
            let p = Point::new(cx + r * angle.cos(), cy + r * angle.sin());
            star_path = if i == 0 {
                star_path.move_to(p)
            } else {
                star_path.line_to(p)
            };
        }
        let star_path = Rc::new(star_path.close());
        children.push(
            Path::new(
                {
                    let d = star_path.clone();
                    move || d.clone()
                },
                || PathStyle {
                    fill: Some(FillStyle::RadialGradient(RadialGradient::new(
                        Point::new(268.0, 403.0),
                        65.0,
                        &[(0.0, WHITE), (0.5, PURPLE), (1.0, DARK)],
                    ))),
                    stroke: Some(Stroke::new(DARK, 1.0)),
                    fill_rule: FillRule::Winding,
                    shadow: None,
                },
            )
            .view(),
        );
        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("star radial"),
            rect: Bounds {
                x: 200.0,
                y: 476.0,
                width: 180.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, MUTED),
        }))));

        let petal = Rc::new(
            PathData::new()
                .move_to(Point::new(452.0, 338.0))
                .cubic_to(
                    Point::new(532.0, 338.0),
                    Point::new(532.0, 468.0),
                    Point::new(452.0, 468.0),
                )
                .cubic_to(
                    Point::new(372.0, 468.0),
                    Point::new(372.0, 338.0),
                    Point::new(452.0, 338.0),
                )
                .close(),
        );
        children.push(
            Path::new(
                {
                    let d = petal.clone();
                    move || d.clone()
                },
                || PathStyle {
                    fill: Some(FillStyle::LinearGradient(LinearGradient::new(
                        Point::new(372.0, 338.0),
                        Point::new(532.0, 468.0),
                        &[
                            (0.0, SUCCESS),
                            (0.5, Color::rgba(0.20, 0.75, 0.90, 1.0)),
                            (1.0, PRIMARY),
                        ],
                    ))),
                    stroke: Some(Stroke::new(DARK, 1.5)),
                    fill_rule: FillRule::Winding,
                    shadow: None,
                },
            )
            .view(),
        );
        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("petal linear 3-stop"),
            rect: Bounds {
                x: 372.0,
                y: 476.0,
                width: 180.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, MUTED),
        }))));

        let rings = Rc::new(
            PathData::new()
                .move_to(Point::new(576.0, 338.0))
                .line_to(Point::new(736.0, 338.0))
                .line_to(Point::new(736.0, 468.0))
                .line_to(Point::new(576.0, 468.0))
                .close()
                .move_to(Point::new(600.0, 362.0))
                .line_to(Point::new(712.0, 362.0))
                .line_to(Point::new(712.0, 444.0))
                .line_to(Point::new(600.0, 444.0))
                .close(),
        );
        children.push(
            Path::new(
                {
                    let d = rings.clone();
                    move || d.clone()
                },
                || PathStyle {
                    fill: Some(FillStyle::LinearGradient(LinearGradient::new(
                        Point::new(576.0, 403.0),
                        Point::new(736.0, 403.0),
                        &[(0.0, DANGER), (1.0, PURPLE)],
                    ))),
                    stroke: None,
                    fill_rule: FillRule::EvenOdd,
                    shadow: None,
                },
            )
            .view(),
        );
        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("even-odd + linear"),
            rect: Bounds {
                x: 576.0,
                y: 476.0,
                width: 180.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, MUTED),
        }))));

        View::group(children)
    })
}

fn layers_section(ctx: &mut WidgetCtx) -> Result<DrawingArea, LayoutError> {
    DrawingArea::new(ctx, LayoutStyle::new().height(560.0), |w, _h| {
        let mut children: Vec<View> = Vec::new();

        children.push(
            Line::new(
                || Point::new(0.0, 0.0),
                move || Point::new(w, 0.0),
                || LineStyle::new(CARD_BORDER, 1.0),
            )
            .view(),
        );
        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("Layers (PushLayer / PopLayer)"),
            rect: Bounds {
                x: 0.0,
                y: 12.0,
                width: 400.0,
                height: 20.0,
            },
            style: TextStyle::new(12.0, MUTED),
        }))));

        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("Opacity — same red rect at 1.0 / 0.6 / 0.3 / 0.1"),
            rect: Bounds {
                x: 0.0,
                y: 40.0,
                width: 500.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, MUTED),
        }))));

        for (i, &opacity) in [1.0f32, 0.6, 0.3, 0.1].iter().enumerate() {
            let x = i as f32 * 184.0;
            children.push(View::Layer {
                opacity,
                children: vec![
                    View::Primitive(DrawCommand::Rect(Box::new(RectPayload {
                        rect: Bounds {
                            x,
                            y: 60.0,
                            width: 168.0,
                            height: 80.0,
                        },
                        style: RectStyle {
                            fill: Some(FillStyle::Solid(DANGER)),
                            stroke: None,
                            radius: BorderRadius::all(8.0),
                            shadow: None,
                        },
                    }))),
                    View::Primitive(DrawCommand::Text(Box::new(TextPayload {
                        text: Rc::from(format!("{opacity:.1}")),
                        rect: Bounds {
                            x,
                            y: 64.0,
                            width: 168.0,
                            height: 72.0,
                        },
                        style: TextStyle::new(18.0, WHITE),
                    }))),
                ],
            });
        }

        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("Overlapping colored layers at 0.7 opacity"),
            rect: Bounds {
                x: 0.0,
                y: 164.0,
                width: 500.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, MUTED),
        }))));

        children.push(View::Primitive(DrawCommand::Rect(Box::new(RectPayload {
            rect: Bounds {
                x: 0.0,
                y: 184.0,
                width: 368.0,
                height: 180.0,
            },
            style: RectStyle {
                fill: Some(FillStyle::Solid(DARK)),
                stroke: None,
                radius: BorderRadius::all(8.0),
                shadow: None,
            },
        }))));

        children.push(View::Layer {
            opacity: 0.7,
            children: vec![View::Primitive(DrawCommand::Rect(Box::new(RectPayload {
                rect: Bounds {
                    x: 16.0,
                    y: 200.0,
                    width: 180.0,
                    height: 120.0,
                },
                style: RectStyle {
                    fill: Some(FillStyle::Solid(PRIMARY)),
                    stroke: None,
                    radius: BorderRadius::all(8.0),
                    shadow: None,
                },
            })))],
        });

        children.push(View::Layer {
            opacity: 0.7,
            children: vec![View::Primitive(DrawCommand::Rect(Box::new(RectPayload {
                rect: Bounds {
                    x: 96.0,
                    y: 240.0,
                    width: 180.0,
                    height: 120.0,
                },
                style: RectStyle {
                    fill: Some(FillStyle::Solid(SUCCESS)),
                    stroke: None,
                    radius: BorderRadius::all(8.0),
                    shadow: None,
                },
            })))],
        });

        children.push(View::Layer {
            opacity: 0.7,
            children: vec![View::Primitive(DrawCommand::Rect(Box::new(RectPayload {
                rect: Bounds {
                    x: 176.0,
                    y: 220.0,
                    width: 180.0,
                    height: 120.0,
                },
                style: RectStyle {
                    fill: Some(FillStyle::Solid(DANGER)),
                    stroke: None,
                    radius: BorderRadius::all(8.0),
                    shadow: None,
                },
            })))],
        });

        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("Layer (0.8) wrapping a gradient rect + text"),
            rect: Bounds {
                x: 396.0,
                y: 164.0,
                width: 360.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, MUTED),
        }))));

        children.push(View::Layer {
            opacity: 0.8,
            children: vec![
                View::Primitive(DrawCommand::Rect(Box::new(RectPayload {
                    rect: Bounds {
                        x: 396.0,
                        y: 184.0,
                        width: 320.0,
                        height: 180.0,
                    },
                    style: RectStyle {
                        fill: Some(FillStyle::LinearGradient(LinearGradient::new(
                            Point::new(396.0, 274.0),
                            Point::new(716.0, 274.0),
                            &[(0.0, PRIMARY), (0.5, PURPLE), (1.0, DANGER)],
                        ))),
                        stroke: None,
                        radius: BorderRadius::all(12.0),
                        shadow: None,
                    },
                }))),
                View::Primitive(DrawCommand::Text(Box::new(TextPayload {
                    text: Rc::from("gradient + layer"),
                    rect: Bounds {
                        x: 396.0,
                        y: 254.0,
                        width: 320.0,
                        height: 60.0,
                    },
                    style: TextStyle::new(18.0, WHITE),
                }))),
            ],
        });

        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("Nested layers: outer 0.6, inner 0.5 → combined ~0.3"),
            rect: Bounds {
                x: 0.0,
                y: 390.0,
                width: 500.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, MUTED),
        }))));

        children.push(View::Layer {
            opacity: 0.6,
            children: vec![
                View::Primitive(DrawCommand::Rect(Box::new(RectPayload {
                    rect: Bounds {
                        x: 0.0,
                        y: 410.0,
                        width: 340.0,
                        height: 120.0,
                    },
                    style: RectStyle {
                        fill: Some(FillStyle::Solid(PRIMARY)),
                        stroke: None,
                        radius: BorderRadius::all(8.0),
                        shadow: None,
                    },
                }))),
                View::Layer {
                    opacity: 0.5,
                    children: vec![
                        View::Primitive(DrawCommand::Rect(Box::new(RectPayload {
                            rect: Bounds {
                                x: 36.0,
                                y: 430.0,
                                width: 260.0,
                                height: 80.0,
                            },
                            style: RectStyle {
                                fill: Some(FillStyle::Solid(DANGER)),
                                stroke: None,
                                radius: BorderRadius::all(6.0),
                                shadow: None,
                            },
                        }))),
                        View::Primitive(DrawCommand::Text(Box::new(TextPayload {
                            text: Rc::from("inner 0.5"),
                            rect: Bounds {
                                x: 36.0,
                                y: 434.0,
                                width: 260.0,
                                height: 72.0,
                            },
                            style: TextStyle::new(14.0, WHITE),
                        }))),
                    ],
                },
                View::Primitive(DrawCommand::Text(Box::new(TextPayload {
                    text: Rc::from("outer 0.6"),
                    rect: Bounds {
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

fn grid_cell(
    ctx: &mut WidgetCtx,
    color: Color,
    label: &'static str,
) -> Result<DrawingArea, LayoutError> {
    DrawingArea::new(ctx, LayoutStyle::new().height(72.0), move |w, h| {
        View::group([
            View::Primitive(DrawCommand::Rect(Box::new(RectPayload {
                rect: Bounds {
                    x: 0.0,
                    y: 0.0,
                    width: w,
                    height: h,
                },
                style: RectStyle {
                    fill: Some(FillStyle::Solid(color)),
                    stroke: None,
                    radius: BorderRadius::all(6.0),
                    shadow: None,
                },
            }))),
            View::Primitive(DrawCommand::Text(Box::new(TextPayload {
                text: Rc::from(label),
                rect: Bounds {
                    x: 0.0,
                    y: 0.0,
                    width: w,
                    height: h,
                },
                style: TextStyle::new(13.0, WHITE),
            }))),
        ])
    })
}

fn grid_section(ctx: &mut WidgetCtx) -> Result<Container, LayoutError> {
    let gc1 = Box::new(grid_cell(ctx, PRIMARY, "1")?) as Box<dyn LayoutItem>;
    let gc2 = Box::new(grid_cell(ctx, SUCCESS, "2")?) as Box<dyn LayoutItem>;
    let gc3 = Box::new(grid_cell(ctx, DANGER, "3")?) as Box<dyn LayoutItem>;
    let gc4 = Box::new(grid_cell(ctx, WARNING, "4")?) as Box<dyn LayoutItem>;
    let gc5 = Box::new(grid_cell(ctx, PURPLE, "5")?) as Box<dyn LayoutItem>;
    let gc6 = Box::new(grid_cell(ctx, DARK, "6")?) as Box<dyn LayoutItem>;
    let auto_grid = Container::new(
        ctx,
        LayoutStyle::new()
            .display_grid()
            .grid_template_columns(vec![Track::repeat(3, Track::fr(1.0))])
            .gap(12.0),
        vec![gc1, gc2, gc3, gc4, gc5, gc6],
    )?;

    let header = DrawingArea::new(
        ctx,
        LayoutStyle::new().height(48.0).grid_column_span(3),
        |w, h| {
            View::group([
                View::Primitive(DrawCommand::Rect(Box::new(RectPayload {
                    rect: Bounds {
                        x: 0.0,
                        y: 0.0,
                        width: w,
                        height: h,
                    },
                    style: RectStyle {
                        fill: Some(FillStyle::Solid(DARK)),
                        stroke: None,
                        radius: BorderRadius::all(6.0),
                        shadow: None,
                    },
                }))),
                View::Primitive(DrawCommand::Text(Box::new(TextPayload {
                    text: Rc::from("header — span 3"),
                    rect: Bounds {
                        x: 0.0,
                        y: 0.0,
                        width: w,
                        height: h,
                    },
                    style: TextStyle::new(13.0, WHITE),
                }))),
            ])
        },
    )?;
    let gca = Box::new(grid_cell(ctx, SUCCESS, "A")?) as Box<dyn LayoutItem>;
    let gcb = Box::new(grid_cell(ctx, DANGER, "B")?) as Box<dyn LayoutItem>;
    let explicit_grid = Container::new(
        ctx,
        LayoutStyle::new()
            .display_grid()
            .grid_template_columns(vec![Track::fr(1.0), Track::fr(1.0), Track::fr(1.0)])
            .gap(12.0),
        vec![Box::new(header) as Box<dyn LayoutItem>, gca, gcb],
    )?;

    let gcg1 = Box::new(grid_cell(ctx, PRIMARY, "G1")?) as Box<dyn LayoutItem>;
    let gcg2 = Box::new(grid_cell(ctx, SUCCESS, "G2")?) as Box<dyn LayoutItem>;
    let gcg3 = Box::new(grid_cell(ctx, DANGER, "G3")?) as Box<dyn LayoutItem>;
    let gcg4 = Box::new(grid_cell(ctx, WARNING, "G4")?) as Box<dyn LayoutItem>;
    let inner_grid = Container::new(
        ctx,
        LayoutStyle::new()
            .display_grid()
            .grid_template_columns(vec![Track::fr(1.0), Track::fr(1.0)])
            .flex_grow(1.0)
            .gap(8.0),
        vec![gcg1, gcg2, gcg3, gcg4],
    )?;
    let side_label = DrawingArea::new(ctx, LayoutStyle::new().width(180.0), |w, h| {
        View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("Grid nested\ninside flex →"),
            rect: Bounds {
                x: 0.0,
                y: 0.0,
                width: w,
                height: h,
            },
            style: TextStyle::new(13.0, MUTED),
        })))
    })?;
    let nested_row = Container::new(
        ctx,
        LayoutStyle::new().flex_row().gap(16.0),
        vec![
            Box::new(side_label) as Box<dyn LayoutItem>,
            Box::new(inner_grid) as Box<dyn LayoutItem>,
        ],
    )?;

    let h1 = heading(ctx, "Grid")?;
    let h2 = heading(ctx, "Auto-placed (repeat(3, 1fr))")?;
    let h3 = heading(ctx, "Explicit placement (grid_column_span)")?;
    let h4 = heading(ctx, "Nested in Container")?;
    Container::new(
        ctx,
        LayoutStyle::new().flex_column().gap(16.0),
        vec![
            h1,
            h2,
            Box::new(auto_grid) as Box<dyn LayoutItem>,
            h3,
            Box::new(explicit_grid) as Box<dyn LayoutItem>,
            h4,
            Box::new(nested_row) as Box<dyn LayoutItem>,
        ],
    )
}

fn shadows_section(ctx: &mut WidgetCtx) -> Result<DrawingArea, LayoutError> {
    DrawingArea::new(ctx, LayoutStyle::new().height(640.0), |w, _h| {
        let mut children: Vec<View> = Vec::new();

        children.push(
            Line::new(
                || Point::new(0.0, 0.0),
                move || Point::new(w, 0.0),
                || LineStyle::new(CARD_BORDER, 1.0),
            )
            .view(),
        );
        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("Shadows"),
            rect: Bounds {
                x: 0.0,
                y: 12.0,
                width: 300.0,
                height: 20.0,
            },
            style: TextStyle::new(12.0, MUTED),
        }))));

        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("Rect shadows — offset / blur / spread"),
            rect: Bounds {
                x: 0.0,
                y: 40.0,
                width: 400.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, MUTED),
        }))));

        children.push(View::Primitive(DrawCommand::Rect(Box::new(RectPayload {
            rect: Bounds {
                x: 0.0,
                y: 60.0,
                width: 152.0,
                height: 80.0,
            },
            style: RectStyle {
                fill: Some(FillStyle::Solid(WHITE)),
                stroke: None,
                radius: BorderRadius::all(8.0),
                shadow: Some(Shadow::new(
                    0.0,
                    4.0,
                    12.0,
                    Color::rgba(0.0, 0.0, 0.0, 0.25),
                )),
            },
        }))));
        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("soft (0, 4, 12)"),
            rect: Bounds {
                x: 0.0,
                y: 146.0,
                width: 152.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, MUTED),
        }))));

        children.push(View::Primitive(DrawCommand::Rect(Box::new(RectPayload {
            rect: Bounds {
                x: 176.0,
                y: 60.0,
                width: 152.0,
                height: 80.0,
            },
            style: RectStyle {
                fill: Some(FillStyle::Solid(WHITE)),
                stroke: None,
                radius: BorderRadius::all(8.0),
                shadow: Some(Shadow::new(4.0, 8.0, 4.0, Color::rgba(0.0, 0.0, 0.0, 0.4))),
            },
        }))));
        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("offset (4, 8, 4)"),
            rect: Bounds {
                x: 176.0,
                y: 146.0,
                width: 152.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, MUTED),
        }))));

        children.push(View::Primitive(DrawCommand::Rect(Box::new(RectPayload {
            rect: Bounds {
                x: 352.0,
                y: 60.0,
                width: 152.0,
                height: 80.0,
            },
            style: RectStyle {
                fill: Some(FillStyle::Solid(WHITE)),
                stroke: None,
                radius: BorderRadius::all(8.0),
                shadow: Some(Shadow::new(
                    0.0,
                    6.0,
                    16.0,
                    Color::rgba(0.24, 0.47, 0.98, 0.5),
                )),
            },
        }))));
        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("colored primary"),
            rect: Bounds {
                x: 352.0,
                y: 146.0,
                width: 152.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, MUTED),
        }))));

        children.push(View::Primitive(DrawCommand::Rect(Box::new(RectPayload {
            rect: Bounds {
                x: 528.0,
                y: 60.0,
                width: 152.0,
                height: 80.0,
            },
            style: RectStyle {
                fill: Some(FillStyle::Solid(WHITE)),
                stroke: None,
                radius: BorderRadius::all(8.0),
                shadow: Some(
                    Shadow::new(0.0, 0.0, 8.0, Color::rgba(0.0, 0.0, 0.0, 0.3)).with_spread(4.0),
                ),
            },
        }))));
        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("spread +4"),
            rect: Bounds {
                x: 528.0,
                y: 146.0,
                width: 152.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, MUTED),
        }))));

        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("Colored shadows on dark cards"),
            rect: Bounds {
                x: 0.0,
                y: 176.0,
                width: 400.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, MUTED),
        }))));

        let card_colors: &[(Color, Color, &str)] = &[
            (PRIMARY, Color::rgba(0.24, 0.47, 0.98, 0.6), "primary glow"),
            (SUCCESS, Color::rgba(0.18, 0.69, 0.45, 0.6), "success glow"),
            (DANGER, Color::rgba(0.92, 0.27, 0.27, 0.6), "danger glow"),
            (PURPLE, Color::rgba(0.60, 0.28, 0.98, 0.6), "purple glow"),
        ];
        for (i, &(card_color, shadow_color, label)) in card_colors.iter().enumerate() {
            let x = i as f32 * 184.0;
            children.push(View::Primitive(DrawCommand::Rect(Box::new(RectPayload {
                rect: Bounds {
                    x,
                    y: 196.0,
                    width: 168.0,
                    height: 80.0,
                },
                style: RectStyle {
                    fill: Some(FillStyle::Solid(card_color)),
                    stroke: None,
                    radius: BorderRadius::all(10.0),
                    shadow: Some(Shadow::new(0.0, 8.0, 20.0, shadow_color)),
                },
            }))));
            children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
                text: Rc::from(label),
                rect: Bounds {
                    x,
                    y: 200.0,
                    width: 168.0,
                    height: 72.0,
                },
                style: TextStyle::new(12.0, WHITE),
            }))));
        }

        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("Text shadows"),
            rect: Bounds {
                x: 0.0,
                y: 312.0,
                width: 300.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, MUTED),
        }))));

        children.push(View::Primitive(DrawCommand::Rect(Box::new(RectPayload {
            rect: Bounds {
                x: 0.0,
                y: 332.0,
                width: 720.0,
                height: 100.0,
            },
            style: RectStyle {
                fill: Some(FillStyle::Solid(WHITE)),
                stroke: Some(Stroke::new(CARD_BORDER, 1.0)),
                radius: BorderRadius::all(8.0),
                shadow: None,
            },
        }))));

        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("Drop shadow"),
            rect: Bounds {
                x: 16.0,
                y: 348.0,
                width: 180.0,
                height: 30.0,
            },
            style: TextStyle::new(22.0, DARK).with_shadow(Shadow::new(
                2.0,
                3.0,
                5.0,
                Color::rgba(0.0, 0.0, 0.0, 0.35),
            )),
        }))));

        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("Color glow"),
            rect: Bounds {
                x: 216.0,
                y: 348.0,
                width: 200.0,
                height: 30.0,
            },
            style: TextStyle::new(22.0, PRIMARY).with_shadow(Shadow::new(
                0.0,
                0.0,
                8.0,
                Color::rgba(0.24, 0.47, 0.98, 0.6),
            )),
        }))));

        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("Hard offset"),
            rect: Bounds {
                x: 436.0,
                y: 348.0,
                width: 200.0,
                height: 30.0,
            },
            style: TextStyle::new(22.0, DARK).with_shadow(Shadow::new(
                3.0,
                3.0,
                1.0,
                Color::rgba(0.92, 0.27, 0.27, 0.7),
            )),
        }))));

        children.push(View::Primitive(DrawCommand::Text(Box::new(TextPayload {
            text: Rc::from("Shadow inside layer"),
            rect: Bounds {
                x: 0.0,
                y: 452.0,
                width: 400.0,
                height: 16.0,
            },
            style: TextStyle::new(11.0, MUTED),
        }))));

        children.push(View::Layer {
            opacity: 1.0,
            children: vec![
                View::Primitive(DrawCommand::Rect(Box::new(RectPayload {
                    rect: Bounds {
                        x: 0.0,
                        y: 472.0,
                        width: 220.0,
                        height: 100.0,
                    },
                    style: RectStyle {
                        fill: Some(FillStyle::Solid(WHITE)),
                        stroke: None,
                        radius: BorderRadius::all(10.0),
                        shadow: Some(Shadow::new(0.0, 6.0, 16.0, Color::rgba(0.0, 0.0, 0.0, 0.2))),
                    },
                }))),
                View::Primitive(DrawCommand::Text(Box::new(TextPayload {
                    text: Rc::from("layer opacity 1.0"),
                    rect: Bounds {
                        x: 16.0,
                        y: 500.0,
                        width: 200.0,
                        height: 20.0,
                    },
                    style: TextStyle::new(12.0, DARK),
                }))),
            ],
        });

        children.push(View::Layer {
            opacity: 0.7,
            children: vec![
                View::Primitive(DrawCommand::Rect(Box::new(RectPayload {
                    rect: Bounds {
                        x: 240.0,
                        y: 472.0,
                        width: 220.0,
                        height: 100.0,
                    },
                    style: RectStyle {
                        fill: Some(FillStyle::Solid(WHITE)),
                        stroke: None,
                        radius: BorderRadius::all(10.0),
                        shadow: Some(Shadow::new(
                            0.0,
                            6.0,
                            16.0,
                            Color::rgba(0.24, 0.47, 0.98, 0.5),
                        )),
                    },
                }))),
                View::Primitive(DrawCommand::Text(Box::new(TextPayload {
                    text: Rc::from("layer opacity 0.7"),
                    rect: Bounds {
                        x: 256.0,
                        y: 500.0,
                        width: 200.0,
                        height: 20.0,
                    },
                    style: TextStyle::new(12.0, DARK),
                }))),
            ],
        });

        children.push(View::Layer {
            opacity: 0.4,
            children: vec![
                View::Primitive(DrawCommand::Rect(Box::new(RectPayload {
                    rect: Bounds {
                        x: 480.0,
                        y: 472.0,
                        width: 220.0,
                        height: 100.0,
                    },
                    style: RectStyle {
                        fill: Some(FillStyle::Solid(WHITE)),
                        stroke: None,
                        radius: BorderRadius::all(10.0),
                        shadow: Some(Shadow::new(
                            4.0,
                            4.0,
                            8.0,
                            Color::rgba(0.6, 0.28, 0.98, 0.6),
                        )),
                    },
                }))),
                View::Primitive(DrawCommand::Text(Box::new(TextPayload {
                    text: Rc::from("layer opacity 0.4"),
                    rect: Bounds {
                        x: 496.0,
                        y: 500.0,
                        width: 200.0,
                        height: 20.0,
                    },
                    style: TextStyle::new(12.0, DARK),
                }))),
            ],
        });

        View::group(children)
    })
}

struct SandboxRootComponent {
    ctx: WidgetCtx,
    content_node: NodeId,
    window_width: RwSignal<f32>,
    window_height: RwSignal<f32>,
    scroll_area: ScrollArea,
}

impl Component for SandboxRootComponent {
    fn view(&self) -> View {
        self.scroll_area.view()
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        if let Event::WindowResized { width, height } = event {
            let w = *width as f32;
            self.window_width.set(w);
            self.window_height.set(*height as f32);
            self.ctx.mark_dirty_node(self.content_node).ok();
            compute_layout(
                &mut self.ctx,
                self.content_node,
                AvailableSpace::Definite(w.max(480.0)),
                AvailableSpace::MaxContent,
            )
            .ok();
            self.scroll_area.clamp_scroll();
            return EventResult::Handled;
        }
        self.scroll_area.on_event(event)
    }
}

struct SandboxRoot;

impl App for SandboxRoot {
    fn root(&self) -> Box<dyn Component> {
        let window_width = create_rw_signal(800.0f32);
        let window_height = create_rw_signal(600.0f32);

        let gradient_image = Rc::new(make_gradient(128, 128));
        let checker_image = Rc::new(make_checker(128, 128, 16));
        let alpha_image = Rc::new(make_radial_alpha(128, 128));

        let (build, ctx) = with_context(WidgetCtx::new(), |ctx| {
            let content = build_content(
                ctx,
                gradient_image.clone(),
                checker_image.clone(),
                alpha_image.clone(),
            )?;

            let content_node = content.layout_node();
            let ww = window_width.clone();
            let wh = window_height.clone();
            let scroll_area = ScrollArea::new(
                ctx,
                move || Bounds::new(0.0, 0.0, ww.get(), wh.get()),
                Box::new(content),
            );

            compute_layout(
                ctx,
                content_node,
                AvailableSpace::Definite(window_width.get()),
                AvailableSpace::MaxContent,
            )?;

            Ok::<_, LayoutError>((scroll_area, content_node))
        });

        let (scroll_area, content_node) = build.expect("layout failed");

        Box::new(SandboxRootComponent {
            ctx,
            content_node,
            window_width,
            window_height,
            scroll_area,
        })
    }

    fn clear_color(&self) -> Option<Color> {
        Some(SURFACE)
    }
}

fn main() {
    tracing_subscriber::fmt::init();
    rsx::run_app!(WindowConfig::default(), SandboxRoot);
}

fn make_gradient(width: u32, height: u32) -> ImageData {
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for _y in 0..height {
        for x in 0..width {
            let t = x as f32 / (width - 1) as f32;
            let r = (t * 255.0) as u8;
            let g = 60u8;
            let b = ((1.0 - t) * 255.0) as u8;
            pixels.extend_from_slice(&[r, g, b, 255]);
        }
    }
    ImageData::new(pixels, width, height)
}

fn make_checker(width: u32, height: u32, cell: u32) -> ImageData {
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let on = ((x / cell) + (y / cell)) % 2 == 0;
            if on {
                pixels.extend_from_slice(&[240, 240, 240, 255]);
            } else {
                pixels.extend_from_slice(&[60, 100, 200, 255]);
            }
        }
    }
    ImageData::new(pixels, width, height)
}

fn make_radial_alpha(width: u32, height: u32) -> ImageData {
    let cx = width as f32 / 2.0;
    let cy = height as f32 / 2.0;
    let radius = cx.min(cy) - 2.0;
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            let alpha = ((radius - dist).clamp(0.0, 1.0) * 255.0) as u8;
            pixels.extend_from_slice(&[240, 140, 30, alpha]);
        }
    }
    ImageData::new(pixels, width, height)
}
