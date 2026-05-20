use std::rc::Rc;

use rsx::{
    AvailableSpace, BorderRadius, Bounds, Button, Color, Component, Event, EventResult, FillRule,
    FillStyle, Image, ImageData, ImageFilter, Label, LayoutStyle, Line, LineCap, LineJoin,
    LineStyle, Path, PathData, PathStyle, Point, ReactiveApp, Rect, RectStyle, RwSignal,
    ScrollDelta, Stroke, SubtreeSlot, Text, TextStyle, View, WidgetCtx, WindowConfig,
    compute_layout, create_rw_signal, new_container, with_context,
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

const CONTENT_HEIGHT: f32 = 1700.0;

const PANEL_X: f32 = 614.0;
const PANEL_Y: f32 = 40.0;
const PANEL_W: f32 = 160.0;
const PANEL_H: f32 = 128.0;

struct WidgetPanel {
    count_label: Label,
    btn_inc: Button,
    btn_dec: Button,
}

impl Component for WidgetPanel {
    fn view(&self) -> View {
        View::group([
            self.count_label.view(),
            self.btn_inc.view(),
            self.btn_dec.view(),
        ])
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        let r1 = self.btn_inc.on_event(event);
        let r2 = self.btn_dec.on_event(event);
        if matches!(r1, EventResult::Handled) || matches!(r2, EventResult::Handled) {
            EventResult::Handled
        } else {
            EventResult::Ignored
        }
    }
}

fn panel_relative_event(event: &Event, dx: f64, dy: f64) -> Option<Event> {
    match event {
        Event::PointerMoved { x, y, source } => Some(Event::PointerMoved {
            x: x - dx,
            y: y - dy,
            source: source.clone(),
        }),
        Event::PointerPressed {
            x,
            y,
            button,
            source,
        } => Some(Event::PointerPressed {
            x: x - dx,
            y: y - dy,
            button: button.clone(),
            source: source.clone(),
        }),
        Event::PointerReleased {
            x,
            y,
            button,
            source,
        } => Some(Event::PointerReleased {
            x: x - dx,
            y: y - dy,
            button: button.clone(),
            source: source.clone(),
        }),
        _ => None,
    }
}

struct StaticContent {
    gradient: Rc<ImageData>,
    checker: Rc<ImageData>,
    alpha: Rc<ImageData>,
}

impl Component for StaticContent {
    fn view(&self) -> View {
        View::group([
            shapes_section(),
            colors_section(),
            typography_section(),
            cards_section(),
            images_section(
                self.gradient.clone(),
                self.checker.clone(),
                self.alpha.clone(),
            ),
            lines_section(),
            paths_section(),
        ])
    }

    fn on_event(&mut self, _: &Event) -> EventResult {
        EventResult::Ignored
    }
}

fn shapes_section() -> View {
    View::Group(vec![
        Text::new(
            "Shapes",
            Bounds::new(24.0, 20.0, 200.0, 20.0),
            TextStyle::new(12.0, MUTED),
        )
        .view(),
        Rect::new(
            Bounds::new(24.0, 44.0, 168.0, 80.0),
            RectStyle {
                fill: Some(FillStyle::Solid(PRIMARY)),
                stroke: None,
                radius: BorderRadius::all(8.0),
            },
        )
        .view(),
        Text::new(
            "fill",
            Bounds::new(24.0, 48.0, 168.0, 72.0),
            TextStyle::new(13.0, WHITE),
        )
        .view(),
        Rect::new(
            Bounds::new(208.0, 44.0, 168.0, 80.0),
            RectStyle {
                fill: None,
                stroke: Some(Stroke::new(DANGER, 2.0)),
                radius: BorderRadius::all(8.0),
            },
        )
        .view(),
        Text::new(
            "stroke",
            Bounds::new(208.0, 48.0, 168.0, 72.0),
            TextStyle::new(13.0, DANGER),
        )
        .view(),
        Rect::new(
            Bounds::new(392.0, 44.0, 168.0, 80.0),
            RectStyle {
                fill: Some(FillStyle::Solid(SUCCESS)),
                stroke: Some(Stroke::new(DARK, 1.5)),
                radius: BorderRadius::zero(),
            },
        )
        .view(),
        Text::new(
            "fill + stroke",
            Bounds::new(392.0, 48.0, 168.0, 72.0),
            TextStyle::new(13.0, WHITE),
        )
        .view(),
        Rect::new(
            Bounds::new(576.0, 44.0, 168.0, 80.0),
            RectStyle {
                fill: Some(FillStyle::Solid(PURPLE)),
                stroke: None,
                radius: BorderRadius::all(40.0),
            },
        )
        .view(),
        Text::new(
            "pill radius",
            Bounds::new(576.0, 48.0, 168.0, 72.0),
            TextStyle::new(13.0, WHITE),
        )
        .view(),
    ])
}

fn colors_section() -> View {
    let mut children: Vec<View> = Vec::new();
    children.push(
        Text::new(
            "Colors",
            Bounds::new(24.0, 148.0, 200.0, 20.0),
            TextStyle::new(12.0, MUTED),
        )
        .view(),
    );

    let swatches = [PRIMARY, SUCCESS, DANGER, WARNING, PURPLE, DARK];
    let labels = ["primary", "success", "danger", "warning", "purple", "dark"];
    for (i, (&color, &label)) in swatches.iter().zip(labels.iter()).enumerate() {
        let x = 24.0 + i as f32 * 116.0;
        children.push(
            Rect::new(
                Bounds::new(x, 172.0, 100.0, 44.0),
                RectStyle {
                    fill: Some(FillStyle::Solid(color)),
                    stroke: None,
                    radius: BorderRadius::all(6.0),
                },
            )
            .view(),
        );
        children.push(
            Text::new(
                label,
                Bounds::new(x, 176.0, 100.0, 36.0),
                TextStyle::new(11.0, WHITE),
            )
            .view(),
        );
    }

    View::Group(children)
}

fn typography_section() -> View {
    View::Group(vec![
        Text::new(
            "Typography",
            Bounds::new(24.0, 240.0, 300.0, 20.0),
            TextStyle::new(12.0, MUTED),
        )
        .view(),
        Text::new(
            "Small — 12px — The quick brown fox",
            Bounds::new(24.0, 262.0, 600.0, 20.0),
            TextStyle::new(12.0, DARK),
        )
        .view(),
        Text::new(
            "Regular — 14px — The quick brown fox",
            Bounds::new(24.0, 286.0, 600.0, 22.0),
            TextStyle::new(14.0, DARK),
        )
        .view(),
        Text::new(
            "Medium — 18px — The quick brown fox",
            Bounds::new(24.0, 312.0, 600.0, 26.0),
            TextStyle::new(18.0, DARK),
        )
        .view(),
        Text::new(
            "Large — 24px — The quick brown fox",
            Bounds::new(24.0, 342.0, 700.0, 32.0),
            TextStyle::new(24.0, DARK),
        )
        .view(),
        Text::new(
            "Display — 32px",
            Bounds::new(24.0, 378.0, 500.0, 42.0),
            TextStyle::new(32.0, PRIMARY),
        )
        .view(),
    ])
}

fn cards_section() -> View {
    View::Group(vec![
        Text::new(
            "Cards",
            Bounds::new(24.0, 440.0, 200.0, 20.0),
            TextStyle::new(12.0, MUTED),
        )
        .view(),
        Rect::new(
            Bounds::new(24.0, 464.0, 368.0, 110.0),
            RectStyle {
                fill: Some(FillStyle::Solid(DARK)),
                stroke: None,
                radius: BorderRadius::all(10.0),
            },
        )
        .view(),
        Text::new(
            "Dark Card",
            Bounds::new(40.0, 478.0, 340.0, 24.0),
            TextStyle::new(16.0, WHITE),
        )
        .view(),
        Text::new(
            "White text on a dark background.",
            Bounds::new(40.0, 508.0, 340.0, 52.0),
            TextStyle::new(13.0, MUTED),
        )
        .view(),
        Rect::new(
            Bounds::new(408.0, 464.0, 368.0, 110.0),
            RectStyle {
                fill: Some(FillStyle::Solid(WHITE)),
                stroke: Some(Stroke::new(CARD_BORDER, 1.0)),
                radius: BorderRadius::all(10.0),
            },
        )
        .view(),
        Text::new(
            "Light Card",
            Bounds::new(424.0, 478.0, 340.0, 24.0),
            TextStyle::new(16.0, DARK),
        )
        .view(),
        Text::new(
            "Dark text on a white background.",
            Bounds::new(424.0, 508.0, 340.0, 52.0),
            TextStyle::new(13.0, MUTED),
        )
        .view(),
    ])
}

fn images_section(gradient: Rc<ImageData>, checker: Rc<ImageData>, alpha: Rc<ImageData>) -> View {
    View::Group(vec![
        Text::new(
            "Images",
            Bounds::new(24.0, 600.0, 200.0, 20.0),
            TextStyle::new(12.0, MUTED),
        )
        .view(),
        Image::new(
            gradient,
            Bounds::new(24.0, 624.0, 128.0, 128.0),
            ImageFilter::Linear,
        )
        .view(),
        Text::new(
            "gradient",
            Bounds::new(24.0, 756.0, 128.0, 16.0),
            TextStyle::new(11.0, MUTED),
        )
        .view(),
        Image::new(
            checker,
            Bounds::new(172.0, 624.0, 192.0, 192.0),
            ImageFilter::Nearest,
        )
        .view(),
        Text::new(
            "checker (scaled)",
            Bounds::new(172.0, 820.0, 192.0, 16.0),
            TextStyle::new(11.0, MUTED),
        )
        .view(),
        Image::new(
            alpha,
            Bounds::new(384.0, 624.0, 128.0, 128.0),
            ImageFilter::Nearest,
        )
        .view(),
        Text::new(
            "alpha blend",
            Bounds::new(384.0, 756.0, 128.0, 16.0),
            TextStyle::new(11.0, MUTED),
        )
        .view(),
    ])
}

fn lines_section() -> View {
    let mut children: Vec<View> = Vec::new();

    children.push(
        Text::new(
            "Lines",
            Bounds::new(24.0, 860.0, 200.0, 20.0),
            TextStyle::new(12.0, MUTED),
        )
        .view(),
    );

    children.push(
        Text::new(
            "Width",
            Bounds::new(24.0, 884.0, 60.0, 16.0),
            TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    let width_examples: &[(f32, &str)] = &[
        (1.0, "1 px"),
        (2.0, "2 px"),
        (4.0, "4 px"),
        (8.0, "8 px"),
        (16.0, "16 px"),
    ];
    let mut cy = 906.0f32;
    for &(w, label) in width_examples {
        children.push(
            Text::new(
                label,
                Bounds::new(24.0, cy - 8.0, 56.0, 16.0),
                TextStyle::new(11.0, MUTED),
            )
            .view(),
        );
        children.push(
            Line::new(
                Point::new(88.0, cy),
                Point::new(360.0, cy),
                LineStyle::new(PRIMARY, w),
            )
            .view(),
        );
        cy += w.max(2.0) + 18.0;
    }

    children.push(
        Text::new(
            "Color",
            Bounds::new(420.0, 884.0, 60.0, 16.0),
            TextStyle::new(11.0, MUTED),
        )
        .view(),
    );
    let color_examples: &[(Color, &str)] = &[
        (PRIMARY, "primary"),
        (SUCCESS, "success"),
        (DANGER, "danger"),
        (WARNING, "warning"),
        (PURPLE, "purple"),
    ];
    for (i, &(color, label)) in color_examples.iter().enumerate() {
        let y = 906.0 + i as f32 * 24.0;
        children.push(
            Line::new(
                Point::new(420.0, y),
                Point::new(680.0, y),
                LineStyle::new(color, 3.0),
            )
            .view(),
        );
        children.push(
            Text::new(
                label,
                Bounds::new(688.0, y - 8.0, 80.0, 16.0),
                TextStyle::new(11.0, color),
            )
            .view(),
        );
    }

    children.push(
        Text::new(
            "Separator & chart",
            Bounds::new(24.0, 1020.0, 300.0, 16.0),
            TextStyle::new(11.0, MUTED),
        )
        .view(),
    );
    children.push(
        Line::new(
            Point::new(24.0, 1040.0),
            Point::new(760.0, 1040.0),
            LineStyle::new(CARD_BORDER, 1.0),
        )
        .view(),
    );

    let ax = 60.0f32;
    let cb = 1150.0f32;
    let ct = 1060.0f32;
    let ax_right = 400.0f32;
    children.push(
        Line::new(
            Point::new(ax, ct),
            Point::new(ax, cb),
            LineStyle::new(MUTED, 1.0),
        )
        .view(),
    );
    children.push(
        Line::new(
            Point::new(ax, cb),
            Point::new(ax_right, cb),
            LineStyle::new(MUTED, 1.0),
        )
        .view(),
    );

    let data_x = [ax, ax + 85.0, ax + 170.0, ax + 255.0, ax_right];
    let s1 = [1140.0f32, 1115.0, 1098.0, 1078.0, 1068.0];
    let s2 = [1135.0f32, 1112.0, 1100.0, 1088.0, 1075.0];
    let s3 = [1115.0f32, 1122.0, 1130.0, 1138.0, 1145.0];
    for i in 0..4 {
        children.push(
            Line::new(
                Point::new(data_x[i], s1[i]),
                Point::new(data_x[i + 1], s1[i + 1]),
                LineStyle::new(PRIMARY, 2.0),
            )
            .view(),
        );
        children.push(
            Line::new(
                Point::new(data_x[i], s2[i]),
                Point::new(data_x[i + 1], s2[i + 1]),
                LineStyle::new(SUCCESS, 2.0),
            )
            .view(),
        );
        children.push(
            Line::new(
                Point::new(data_x[i], s3[i]),
                Point::new(data_x[i + 1], s3[i + 1]),
                LineStyle::new(DANGER, 2.0),
            )
            .view(),
        );
    }

    children.push(
        Text::new(
            "Diagonals",
            Bounds::new(460.0, 1044.0, 120.0, 16.0),
            TextStyle::new(11.0, MUTED),
        )
        .view(),
    );
    let fan_cx = 590.0f32;
    let fan_cy = 1110.0f32;
    let fan_tips: &[(f32, f32, Color)] = &[
        (500.0, 1064.0, PRIMARY),
        (540.0, 1058.0, SUCCESS),
        (590.0, 1058.0, DANGER),
        (640.0, 1058.0, WARNING),
        (680.0, 1064.0, PURPLE),
        (710.0, 1082.0, DARK),
    ];
    for &(tx, ty, color) in fan_tips {
        children.push(
            Line::new(
                Point::new(fan_cx, fan_cy),
                Point::new(tx, ty),
                LineStyle::new(color, 2.0).with_cap(LineCap::Round),
            )
            .view(),
        );
    }

    View::Group(children)
}

fn paths_section() -> View {
    const Y0: f32 = 1200.0;
    let mut children: Vec<View> = Vec::new();

    children.push(
        Line::new(
            Point::new(24.0, Y0),
            Point::new(760.0, Y0),
            LineStyle::new(CARD_BORDER, 1.0),
        )
        .view(),
    );
    children.push(
        Text::new(
            "Paths",
            Bounds::new(24.0, Y0 + 12.0, 200.0, 20.0),
            TextStyle::new(12.0, MUTED),
        )
        .view(),
    );
    children.push(
        Text::new(
            "Polygon shapes",
            Bounds::new(24.0, Y0 + 36.0, 300.0, 16.0),
            TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    children.push(
        Path::new(
            Rc::new(
                PathData::new()
                    .move_to(Point::new(99.0, Y0 + 56.0))
                    .line_to(Point::new(159.0, Y0 + 166.0))
                    .line_to(Point::new(39.0, Y0 + 166.0))
                    .close(),
            ),
            PathStyle {
                fill: Some(FillStyle::Solid(PRIMARY)),
                stroke: None,
                fill_rule: FillRule::Winding,
            },
        )
        .view(),
    );
    children.push(
        Text::new(
            "triangle",
            Bounds::new(24.0, Y0 + 176.0, 150.0, 16.0),
            TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    {
        let cx = 269.0f32;
        let cy = Y0 + 111.0;
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
        children.push(
            Path::new(
                Rc::new(path),
                PathStyle {
                    fill: Some(FillStyle::Solid(DANGER)),
                    stroke: Some(Stroke::new(DARK, 1.0)),
                    fill_rule: FillRule::Winding,
                },
            )
            .view(),
        );
        children.push(
            Text::new(
                "star (fill + stroke)",
                Bounds::new(199.0, Y0 + 176.0, 200.0, 16.0),
                TextStyle::new(11.0, MUTED),
            )
            .view(),
        );
    }

    children.push(
        Path::new(
            Rc::new(
                PathData::new()
                    .move_to(Point::new(384.0, Y0 + 58.0))
                    .line_to(Point::new(564.0, Y0 + 58.0))
                    .line_to(Point::new(564.0, Y0 + 168.0))
                    .line_to(Point::new(384.0, Y0 + 168.0))
                    .close()
                    .move_to(Point::new(414.0, Y0 + 88.0))
                    .line_to(Point::new(534.0, Y0 + 88.0))
                    .line_to(Point::new(534.0, Y0 + 138.0))
                    .line_to(Point::new(414.0, Y0 + 138.0))
                    .close(),
            ),
            PathStyle {
                fill: Some(FillStyle::Solid(PURPLE)),
                stroke: None,
                fill_rule: FillRule::EvenOdd,
            },
        )
        .view(),
    );
    children.push(
        Text::new(
            "even-odd fill",
            Bounds::new(374.0, Y0 + 176.0, 200.0, 16.0),
            TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    children.push(
        Text::new(
            "Bézier curves",
            Bounds::new(24.0, Y0 + 212.0, 300.0, 16.0),
            TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    children.push(
        Path::new(
            Rc::new(
                PathData::new()
                    .move_to(Point::new(24.0, Y0 + 308.0))
                    .quad_to(Point::new(164.0, Y0 + 238.0), Point::new(304.0, Y0 + 308.0)),
            ),
            PathStyle {
                fill: None,
                stroke: Some(Stroke::new(WARNING, 3.0).with_cap(LineCap::Round)),
                fill_rule: FillRule::Winding,
            },
        )
        .view(),
    );
    children.push(
        Text::new(
            "quad_to arch",
            Bounds::new(24.0, Y0 + 318.0, 200.0, 16.0),
            TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    children.push(
        Path::new(
            Rc::new(
                PathData::new()
                    .move_to(Point::new(334.0, Y0 + 248.0))
                    .cubic_to(
                        Point::new(404.0, Y0 + 248.0),
                        Point::new(334.0, Y0 + 308.0),
                        Point::new(404.0, Y0 + 308.0),
                    ),
            ),
            PathStyle {
                fill: None,
                stroke: Some(Stroke::new(SUCCESS, 3.0).with_cap(LineCap::Round)),
                fill_rule: FillRule::Winding,
            },
        )
        .view(),
    );
    children.push(
        Text::new(
            "cubic_to S-curve",
            Bounds::new(320.0, Y0 + 318.0, 200.0, 16.0),
            TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    children.push(
        Path::new(
            Rc::new(
                PathData::new()
                    .move_to(Point::new(540.0, Y0 + 243.0))
                    .cubic_to(
                        Point::new(610.0, Y0 + 243.0),
                        Point::new(610.0, Y0 + 313.0),
                        Point::new(540.0, Y0 + 313.0),
                    )
                    .cubic_to(
                        Point::new(470.0, Y0 + 313.0),
                        Point::new(470.0, Y0 + 243.0),
                        Point::new(540.0, Y0 + 243.0),
                    )
                    .close(),
            ),
            PathStyle {
                fill: Some(FillStyle::Solid(Color::rgba(0.97, 0.72, 0.18, 0.75))),
                stroke: Some(Stroke::new(WARNING, 1.5)),
                fill_rule: FillRule::Winding,
            },
        )
        .view(),
    );
    children.push(
        Text::new(
            "closed cubic (petal)",
            Bounds::new(470.0, Y0 + 318.0, 200.0, 16.0),
            TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    children.push(
        Text::new(
            "Stroke style",
            Bounds::new(24.0, Y0 + 354.0, 300.0, 16.0),
            TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    children.push(
        Path::new(
            Rc::new(
                PathData::new()
                    .move_to(Point::new(24.0, Y0 + 410.0))
                    .line_to(Point::new(100.0, Y0 + 390.0))
                    .line_to(Point::new(176.0, Y0 + 430.0))
                    .line_to(Point::new(252.0, Y0 + 390.0)),
            ),
            PathStyle {
                fill: None,
                stroke: Some(Stroke::new(PRIMARY, 8.0)),
                fill_rule: FillRule::Winding,
            },
        )
        .view(),
    );
    children.push(
        Text::new(
            "Butt / Miter (default)",
            Bounds::new(24.0, Y0 + 448.0, 230.0, 16.0),
            TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    children.push(
        Path::new(
            Rc::new(
                PathData::new()
                    .move_to(Point::new(324.0, Y0 + 410.0))
                    .line_to(Point::new(400.0, Y0 + 390.0))
                    .line_to(Point::new(476.0, Y0 + 430.0))
                    .line_to(Point::new(552.0, Y0 + 390.0)),
            ),
            PathStyle {
                fill: None,
                stroke: Some(
                    Stroke::new(DANGER, 8.0)
                        .with_cap(LineCap::Round)
                        .with_join(LineJoin::Round),
                ),
                fill_rule: FillRule::Winding,
            },
        )
        .view(),
    );
    children.push(
        Text::new(
            "Round cap / Round join",
            Bounds::new(324.0, Y0 + 448.0, 240.0, 16.0),
            TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    View::Group(children)
}

struct SandboxRootComponent {
    scroll_y: RwSignal<f32>,
    window_width: RwSignal<f32>,
    window_height: RwSignal<f32>,
    static_content: SubtreeSlot,
    widget_panel: SubtreeSlot,
}

impl Component for SandboxRootComponent {
    fn view(&self) -> View {
        let _widget_ver = self.widget_panel.version().get();

        let scroll_y = self.scroll_y.get();
        let window_width = self.window_width.get();
        let window_height = self.window_height.get();

        let scrollable = View::Clip {
            rect: Bounds::new(0.0, 0.0, f32::MAX, window_height),
            children: vec![View::Translate {
                tx: 0.0,
                ty: -scroll_y,
                children: vec![View::Subtree(self.static_content.handle())],
            }],
        };

        let widget_label = Text::new(
            "Reactive Widgets",
            Bounds::new(PANEL_X, PANEL_Y - 18.0, PANEL_W, 14.0),
            TextStyle::new(11.0, MUTED),
        )
        .view();
        let panel_bg = Rect::new(
            Bounds::new(0.0, 0.0, PANEL_W, PANEL_H),
            RectStyle {
                fill: Some(FillStyle::Solid(DARK)),
                stroke: Some(Stroke::new(CARD_BORDER, 1.0)),
                radius: BorderRadius::all(8.0),
            },
        )
        .view();
        let widget_area = View::Translate {
            tx: PANEL_X,
            ty: PANEL_Y,
            children: vec![panel_bg, View::Subtree(self.widget_panel.handle())],
        };

        let scrollbar = if CONTENT_HEIGHT > window_height {
            let bar_h = (window_height / CONTENT_HEIGHT * window_height).max(24.0);
            let max_scroll = (CONTENT_HEIGHT - window_height).max(1.0);
            let bar_y = (scroll_y / max_scroll) * (window_height - bar_h);
            Rect::new(
                Bounds::new(window_width - 8.0, bar_y, 6.0, bar_h),
                RectStyle {
                    fill: Some(FillStyle::Solid(MUTED)),
                    stroke: None,
                    radius: BorderRadius::all(3.0),
                },
            )
            .view()
        } else {
            View::Empty
        };

        View::Group(vec![widget_label, scrollable, widget_area, scrollbar])
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        let mut handled = EventResult::Ignored;

        if let Some(rel) = panel_relative_event(event, PANEL_X as f64, PANEL_Y as f64)
            && self.widget_panel.on_event(&rel) == EventResult::Handled
        {
            handled = EventResult::Handled;
        }

        if let Event::Scrolled { delta } = event {
            let dy = match delta {
                ScrollDelta::Lines { y, .. } => *y * 20.0,
                ScrollDelta::Pixels { y, .. } => *y,
            };
            let window_height = self.window_height.get();
            let max_scroll = (CONTENT_HEIGHT - window_height).max(0.0);
            let new_scroll = (self.scroll_y.get() - dy).clamp(0.0, max_scroll);
            self.scroll_y.set(new_scroll);
            handled = EventResult::Handled;
        }

        if let Event::WindowResized { width, height } = event {
            self.window_width.set(*width as f32);
            self.window_height.set(*height as f32);
            handled = EventResult::Handled;
        }

        handled
    }
}

struct SandboxRoot;

impl ReactiveApp for SandboxRoot {
    fn root(&self) -> Box<dyn Component> {
        let scroll_y = create_rw_signal(0.0f32);
        let window_width = create_rw_signal(800.0f32);
        let window_height = create_rw_signal(600.0f32);

        let gradient_image = Rc::new(make_gradient(128, 128));
        let checker_image = Rc::new(make_checker(128, 128, 16));
        let alpha_image = Rc::new(make_radial_alpha(128, 128));

        let count = create_rw_signal(0i32);

        let ((count_label, btn_inc, btn_dec), _) = with_context(WidgetCtx::new(), || {
            let c = count.clone();
            let count_label = Label::from_fn_with_size(
                move || format!("Count: {}", c.get()),
                PANEL_W - 16.0,
                24.0,
                TextStyle::new(14.0, WHITE),
            )
            .expect("layout failed");
            let btn_inc = Button::new("+").expect("layout failed");
            let btn_dec = Button::new("-").expect("layout failed");
            let widget_root = new_container(
                LayoutStyle::new()
                    .flex_column()
                    .width(PANEL_W)
                    .height(PANEL_H)
                    .padding_all(8.0)
                    .gap(8.0),
                &[
                    count_label.layout_node(),
                    btn_inc.layout_node(),
                    btn_dec.layout_node(),
                ],
            )
            .expect("layout failed");
            compute_layout(
                widget_root,
                AvailableSpace::Definite(PANEL_W),
                AvailableSpace::Definite(PANEL_H),
            )
            .expect("layout failed");
            (count_label, btn_inc, btn_dec)
        });

        let c = count.clone();
        let btn_inc = btn_inc
            .with_bg(
                Color::from_rgb_u8(34, 197, 94),
                Color::from_rgb_u8(22, 163, 74),
            )
            .on_click(move || c.set(c.get() + 1));

        let c = count.clone();
        let btn_dec = btn_dec
            .with_bg(
                Color::from_rgb_u8(239, 68, 68),
                Color::from_rgb_u8(220, 38, 38),
            )
            .on_click(move || c.set(c.get() - 1));

        Box::new(SandboxRootComponent {
            scroll_y,
            window_width,
            window_height,
            static_content: SubtreeSlot::new(StaticContent {
                gradient: gradient_image,
                checker: checker_image,
                alpha: alpha_image,
            }),
            widget_panel: SubtreeSlot::new(WidgetPanel {
                count_label,
                btn_inc,
                btn_dec,
            }),
        })
    }

    fn clear_color(&self) -> Option<Color> {
        Some(SURFACE)
    }
}

fn main() {
    tracing_subscriber::fmt::init();
    rsx::run_reactive!(WindowConfig::default(), SandboxRoot);
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
