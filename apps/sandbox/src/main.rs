use std::rc::Rc;

use rsx::{
    App, AvailableSpace, BorderRadius, Bounds, Button, Color, Component, Event, EventResult,
    FillRule, FillStyle, Image, ImageData, ImageFilter, Label, LayoutStyle, Line, LineCap,
    LineJoin, LineStyle, LinearGradient, Path, PathData, PathStyle, Point, RadialGradient, Rect,
    RectStyle, RwSignal, ScrollArea, Shadow, Stroke, Text, TextStyle, TranslateGroup, View,
    WidgetCtx, WindowConfig, compute_layout, create_rw_signal, new_container, with_context,
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

const CONTENT_HEIGHT: f32 = 3600.0;

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
        self.btn_inc
            .on_event(event)
            .or(self.btn_dec.on_event(event))
    }
}

fn static_content(gradient: Rc<ImageData>, checker: Rc<ImageData>, alpha: Rc<ImageData>) -> View {
    const PATHS_Y: f32 = 1200.0;
    const PATHS_H: f32 = 660.0;
    const GRADIENTS_H: f32 = 520.0;
    const LAYERS_H: f32 = 560.0;

    let gradients_y = PATHS_Y + PATHS_H;
    let layers_y = gradients_y + GRADIENTS_H;
    let shadows_y = layers_y + LAYERS_H;

    View::group([
        shapes_section(),
        colors_section(),
        typography_section(),
        cards_section(),
        images_section(gradient, checker, alpha),
        lines_section(),
        paths_section(PATHS_Y),
        gradients_section(gradients_y),
        layers_section(layers_y),
        shadows_section(shadows_y),
    ])
}

fn shapes_section() -> View {
    View::group([
        Text::new(
            || "Shapes".to_string(),
            || Bounds::new(24.0, 20.0, 200.0, 20.0),
            || TextStyle::new(12.0, MUTED),
        )
        .view(),
        Rect::new(
            || Bounds::new(24.0, 44.0, 168.0, 80.0),
            || RectStyle {
                fill: Some(FillStyle::Solid(PRIMARY)),
                stroke: None,
                radius: BorderRadius::all(8.0),
                shadow: None,
            },
        )
        .view(),
        Text::new(
            || "fill".to_string(),
            || Bounds::new(24.0, 48.0, 168.0, 72.0),
            || TextStyle::new(13.0, WHITE),
        )
        .view(),
        Rect::new(
            || Bounds::new(208.0, 44.0, 168.0, 80.0),
            || RectStyle {
                fill: None,
                stroke: Some(Stroke::new(DANGER, 2.0)),
                radius: BorderRadius::all(8.0),
                shadow: None,
            },
        )
        .view(),
        Text::new(
            || "stroke".to_string(),
            || Bounds::new(208.0, 48.0, 168.0, 72.0),
            || TextStyle::new(13.0, DANGER),
        )
        .view(),
        Rect::new(
            || Bounds::new(392.0, 44.0, 168.0, 80.0),
            || RectStyle {
                fill: Some(FillStyle::Solid(SUCCESS)),
                stroke: Some(Stroke::new(DARK, 1.5)),
                radius: BorderRadius::zero(),
                shadow: None,
            },
        )
        .view(),
        Text::new(
            || "fill + stroke".to_string(),
            || Bounds::new(392.0, 48.0, 168.0, 72.0),
            || TextStyle::new(13.0, WHITE),
        )
        .view(),
        Rect::new(
            || Bounds::new(576.0, 44.0, 168.0, 80.0),
            || RectStyle {
                fill: Some(FillStyle::Solid(PURPLE)),
                stroke: None,
                radius: BorderRadius::all(40.0),
                shadow: None,
            },
        )
        .view(),
        Text::new(
            || "pill radius".to_string(),
            || Bounds::new(576.0, 48.0, 168.0, 72.0),
            || TextStyle::new(13.0, WHITE),
        )
        .view(),
    ])
}

fn colors_section() -> View {
    let mut children: Vec<View> = Vec::new();
    children.push(
        Text::new(
            || "Colors".to_string(),
            || Bounds::new(24.0, 148.0, 200.0, 20.0),
            || TextStyle::new(12.0, MUTED),
        )
        .view(),
    );

    let swatches = [PRIMARY, SUCCESS, DANGER, WARNING, PURPLE, DARK];
    let labels = ["primary", "success", "danger", "warning", "purple", "dark"];
    for (i, (&color, &label)) in swatches.iter().zip(labels.iter()).enumerate() {
        let x = 24.0 + i as f32 * 116.0;
        children.push(
            Rect::new(
                move || Bounds::new(x, 172.0, 100.0, 44.0),
                move || RectStyle {
                    fill: Some(FillStyle::Solid(color)),
                    stroke: None,
                    radius: BorderRadius::all(6.0),
                    shadow: None,
                },
            )
            .view(),
        );
        children.push(
            Text::new(
                || label.to_string(),
                move || Bounds::new(x, 176.0, 100.0, 36.0),
                || TextStyle::new(11.0, WHITE),
            )
            .view(),
        );
    }

    View::group(children)
}

fn typography_section() -> View {
    View::group([
        Text::new(
            || "Typography".to_string(),
            || Bounds::new(24.0, 240.0, 300.0, 20.0),
            || TextStyle::new(12.0, MUTED),
        )
        .view(),
        Text::new(
            || "Small — 12px — The quick brown fox".to_string(),
            || Bounds::new(24.0, 262.0, 600.0, 20.0),
            || TextStyle::new(12.0, DARK),
        )
        .view(),
        Text::new(
            || "Regular — 14px — The quick brown fox".to_string(),
            || Bounds::new(24.0, 286.0, 600.0, 22.0),
            || TextStyle::new(14.0, DARK),
        )
        .view(),
        Text::new(
            || "Medium — 18px — The quick brown fox".to_string(),
            || Bounds::new(24.0, 312.0, 600.0, 26.0),
            || TextStyle::new(18.0, DARK),
        )
        .view(),
        Text::new(
            || "Large — 24px — The quick brown fox".to_string(),
            || Bounds::new(24.0, 342.0, 700.0, 32.0),
            || TextStyle::new(24.0, DARK),
        )
        .view(),
        Text::new(
            || "Display — 32px".to_string(),
            || Bounds::new(24.0, 378.0, 500.0, 42.0),
            || TextStyle::new(32.0, PRIMARY),
        )
        .view(),
    ])
}

fn cards_section() -> View {
    View::group([
        Text::new(
            || "Cards".to_string(),
            || Bounds::new(24.0, 440.0, 200.0, 20.0),
            || TextStyle::new(12.0, MUTED),
        )
        .view(),
        Rect::new(
            || Bounds::new(24.0, 464.0, 368.0, 110.0),
            || RectStyle {
                fill: Some(FillStyle::Solid(DARK)),
                stroke: None,
                radius: BorderRadius::all(10.0),
                shadow: None,
            },
        )
        .view(),
        Text::new(
            || "Dark Card".to_string(),
            || Bounds::new(40.0, 478.0, 340.0, 24.0),
            || TextStyle::new(16.0, WHITE),
        )
        .view(),
        Text::new(
            || "White text on a dark background.".to_string(),
            || Bounds::new(40.0, 508.0, 340.0, 52.0),
            || TextStyle::new(13.0, MUTED),
        )
        .view(),
        Rect::new(
            || Bounds::new(408.0, 464.0, 368.0, 110.0),
            || RectStyle {
                fill: Some(FillStyle::Solid(WHITE)),
                stroke: Some(Stroke::new(CARD_BORDER, 1.0)),
                radius: BorderRadius::all(10.0),
                shadow: None,
            },
        )
        .view(),
        Text::new(
            || "Light Card".to_string(),
            || Bounds::new(424.0, 478.0, 340.0, 24.0),
            || TextStyle::new(16.0, DARK),
        )
        .view(),
        Text::new(
            || "Dark text on a white background.".to_string(),
            || Bounds::new(424.0, 508.0, 340.0, 52.0),
            || TextStyle::new(13.0, MUTED),
        )
        .view(),
    ])
}

fn images_section(gradient: Rc<ImageData>, checker: Rc<ImageData>, alpha: Rc<ImageData>) -> View {
    View::group([
        Text::new(
            || "Images".to_string(),
            || Bounds::new(24.0, 600.0, 200.0, 20.0),
            || TextStyle::new(12.0, MUTED),
        )
        .view(),
        Image::new(
            {
                let g = gradient.clone();
                move || g.clone()
            },
            || Bounds::new(24.0, 624.0, 128.0, 128.0),
            || ImageFilter::Linear,
        )
        .view(),
        Text::new(
            || "gradient".to_string(),
            || Bounds::new(24.0, 756.0, 128.0, 16.0),
            || TextStyle::new(11.0, MUTED),
        )
        .view(),
        Image::new(
            {
                let c = checker.clone();
                move || c.clone()
            },
            || Bounds::new(172.0, 624.0, 192.0, 192.0),
            || ImageFilter::Nearest,
        )
        .view(),
        Text::new(
            || "checker (scaled)".to_string(),
            || Bounds::new(172.0, 820.0, 192.0, 16.0),
            || TextStyle::new(11.0, MUTED),
        )
        .view(),
        Image::new(
            {
                let a = alpha.clone();
                move || a.clone()
            },
            || Bounds::new(384.0, 624.0, 128.0, 128.0),
            || ImageFilter::Nearest,
        )
        .view(),
        Text::new(
            || "alpha blend".to_string(),
            || Bounds::new(384.0, 756.0, 128.0, 16.0),
            || TextStyle::new(11.0, MUTED),
        )
        .view(),
    ])
}

fn lines_section() -> View {
    let mut children: Vec<View> = Vec::new();

    children.push(
        Text::new(
            || "Lines".to_string(),
            || Bounds::new(24.0, 860.0, 200.0, 20.0),
            || TextStyle::new(12.0, MUTED),
        )
        .view(),
    );

    children.push(
        Text::new(
            || "Width".to_string(),
            || Bounds::new(24.0, 884.0, 60.0, 16.0),
            || TextStyle::new(11.0, MUTED),
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
                || label.to_string(),
                move || Bounds::new(24.0, cy - 8.0, 56.0, 16.0),
                || TextStyle::new(11.0, MUTED),
            )
            .view(),
        );
        children.push(
            Line::new(
                move || Point::new(88.0, cy),
                move || Point::new(360.0, cy),
                move || LineStyle::new(PRIMARY, w),
            )
            .view(),
        );
        cy += w.max(2.0) + 18.0;
    }

    children.push(
        Text::new(
            || "Color".to_string(),
            || Bounds::new(420.0, 884.0, 60.0, 16.0),
            || TextStyle::new(11.0, MUTED),
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
                move || Point::new(420.0, y),
                move || Point::new(680.0, y),
                move || LineStyle::new(color, 3.0),
            )
            .view(),
        );
        children.push(
            Text::new(
                || label.to_string(),
                move || Bounds::new(688.0, y - 8.0, 80.0, 16.0),
                move || TextStyle::new(11.0, color),
            )
            .view(),
        );
    }

    children.push(
        Text::new(
            || "Separator & chart".to_string(),
            || Bounds::new(24.0, 1020.0, 300.0, 16.0),
            || TextStyle::new(11.0, MUTED),
        )
        .view(),
    );
    children.push(
        Line::new(
            || Point::new(24.0, 1040.0),
            || Point::new(760.0, 1040.0),
            || LineStyle::new(CARD_BORDER, 1.0),
        )
        .view(),
    );

    let ax = 60.0f32;
    let cb = 1150.0f32;
    let ct = 1060.0f32;
    let ax_right = 400.0f32;
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
    let s1 = [1140.0f32, 1115.0, 1098.0, 1078.0, 1068.0];
    let s2 = [1135.0f32, 1112.0, 1100.0, 1088.0, 1075.0];
    let s3 = [1115.0f32, 1122.0, 1130.0, 1138.0, 1145.0];
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

    children.push(
        Text::new(
            || "Diagonals".to_string(),
            || Bounds::new(460.0, 1044.0, 120.0, 16.0),
            || TextStyle::new(11.0, MUTED),
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
                move || Point::new(fan_cx, fan_cy),
                move || Point::new(tx, ty),
                move || LineStyle::new(color, 2.0).with_cap(LineCap::Round),
            )
            .view(),
        );
    }

    View::group(children)
}

fn paths_section(y0: f32) -> View {
    let mut children: Vec<View> = Vec::new();

    children.push(
        Line::new(
            move || Point::new(24.0, y0),
            move || Point::new(760.0, y0),
            || LineStyle::new(CARD_BORDER, 1.0),
        )
        .view(),
    );
    children.push(
        Text::new(
            || "Paths".to_string(),
            move || Bounds::new(24.0, y0 + 12.0, 200.0, 20.0),
            || TextStyle::new(12.0, MUTED),
        )
        .view(),
    );
    children.push(
        Text::new(
            || "Polygon shapes".to_string(),
            move || Bounds::new(24.0, y0 + 36.0, 300.0, 16.0),
            || TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    let triangle_data = Rc::new(
        PathData::new()
            .move_to(Point::new(99.0, y0 + 56.0))
            .line_to(Point::new(159.0, y0 + 166.0))
            .line_to(Point::new(39.0, y0 + 166.0))
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
    children.push(
        Text::new(
            || "triangle".to_string(),
            move || Bounds::new(24.0, y0 + 176.0, 150.0, 16.0),
            || TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    {
        let cx = 269.0f32;
        let cy = y0 + 111.0;
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
        children.push(
            Text::new(
                || "star (fill + stroke)".to_string(),
                move || Bounds::new(199.0, y0 + 176.0, 200.0, 16.0),
                || TextStyle::new(11.0, MUTED),
            )
            .view(),
        );
    }

    let evenodd_data = Rc::new(
        PathData::new()
            .move_to(Point::new(384.0, y0 + 58.0))
            .line_to(Point::new(564.0, y0 + 58.0))
            .line_to(Point::new(564.0, y0 + 168.0))
            .line_to(Point::new(384.0, y0 + 168.0))
            .close()
            .move_to(Point::new(414.0, y0 + 88.0))
            .line_to(Point::new(534.0, y0 + 88.0))
            .line_to(Point::new(534.0, y0 + 138.0))
            .line_to(Point::new(414.0, y0 + 138.0))
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
    children.push(
        Text::new(
            || "even-odd fill".to_string(),
            move || Bounds::new(374.0, y0 + 176.0, 200.0, 16.0),
            || TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    children.push(
        Text::new(
            || "Bézier curves".to_string(),
            move || Bounds::new(24.0, y0 + 212.0, 300.0, 16.0),
            || TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    let quad_data = Rc::new(
        PathData::new()
            .move_to(Point::new(24.0, y0 + 308.0))
            .quad_to(Point::new(164.0, y0 + 238.0), Point::new(304.0, y0 + 308.0)),
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
    children.push(
        Text::new(
            || "quad_to arch".to_string(),
            move || Bounds::new(24.0, y0 + 318.0, 200.0, 16.0),
            || TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    let cubic_data = Rc::new(
        PathData::new()
            .move_to(Point::new(334.0, y0 + 248.0))
            .cubic_to(
                Point::new(404.0, y0 + 248.0),
                Point::new(334.0, y0 + 308.0),
                Point::new(404.0, y0 + 308.0),
            ),
    );
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
    children.push(
        Text::new(
            || "cubic_to S-curve".to_string(),
            move || Bounds::new(320.0, y0 + 318.0, 200.0, 16.0),
            || TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    let petal_data = Rc::new(
        PathData::new()
            .move_to(Point::new(540.0, y0 + 243.0))
            .cubic_to(
                Point::new(610.0, y0 + 243.0),
                Point::new(610.0, y0 + 313.0),
                Point::new(540.0, y0 + 313.0),
            )
            .cubic_to(
                Point::new(470.0, y0 + 313.0),
                Point::new(470.0, y0 + 243.0),
                Point::new(540.0, y0 + 243.0),
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
    children.push(
        Text::new(
            || "closed cubic (petal)".to_string(),
            move || Bounds::new(470.0, y0 + 318.0, 200.0, 16.0),
            || TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    children.push(
        Text::new(
            || "Stroke style".to_string(),
            move || Bounds::new(24.0, y0 + 354.0, 300.0, 16.0),
            || TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    let butt_data = Rc::new(
        PathData::new()
            .move_to(Point::new(24.0, y0 + 410.0))
            .line_to(Point::new(100.0, y0 + 390.0))
            .line_to(Point::new(176.0, y0 + 430.0))
            .line_to(Point::new(252.0, y0 + 390.0)),
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
    children.push(
        Text::new(
            || "Butt / Miter (default)".to_string(),
            move || Bounds::new(24.0, y0 + 448.0, 230.0, 16.0),
            || TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    let round_data = Rc::new(
        PathData::new()
            .move_to(Point::new(324.0, y0 + 410.0))
            .line_to(Point::new(400.0, y0 + 390.0))
            .line_to(Point::new(476.0, y0 + 430.0))
            .line_to(Point::new(552.0, y0 + 390.0)),
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
    children.push(
        Text::new(
            || "Round cap / Round join".to_string(),
            move || Bounds::new(324.0, y0 + 448.0, 240.0, 16.0),
            || TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    children.push(
        Text::new(
            || "Path shadows".to_string(),
            move || Bounds::new(24.0, y0 + 490.0, 300.0, 16.0),
            || TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    const K: f32 = 0.5523;
    let (cx1, cy1, r1) = (100.0_f32, y0 + 570.0, 44.0_f32);
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
    children.push(
        Text::new(
            || "drop shadow".to_string(),
            move || Bounds::new(56.0, y0 + 624.0, 88.0, 16.0),
            || TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    let star_shadow_data = Rc::new({
        let cx = 296.0_f32;
        let cy = y0 + 570.0;
        let outer = 44.0_f32;
        let inner = 18.0_f32;
        let n = 5usize;
        let mut pd = PathData::new();
        for i in 0..n * 2 {
            let r = if i % 2 == 0 { outer } else { inner };
            let angle = std::f32::consts::PI * i as f32 / n as f32 - std::f32::consts::FRAC_PI_2;
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
    children.push(
        Text::new(
            || "glow".to_string(),
            move || Bounds::new(272.0, y0 + 624.0, 48.0, 16.0),
            || TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    let (cx3, cy3, r3) = (492.0_f32, y0 + 570.0, 44.0_f32);
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
    children.push(
        Text::new(
            || "hard offset".to_string(),
            move || Bounds::new(452.0, y0 + 624.0, 80.0, 16.0),
            || TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    let wave_data = Rc::new(
        PathData::new()
            .move_to(Point::new(580.0, y0 + 570.0))
            .cubic_to(
                Point::new(610.0, y0 + 530.0),
                Point::new(650.0, y0 + 530.0),
                Point::new(680.0, y0 + 570.0),
            )
            .cubic_to(
                Point::new(710.0, y0 + 610.0),
                Point::new(750.0, y0 + 610.0),
                Point::new(780.0, y0 + 570.0),
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
    children.push(
        Text::new(
            || "stroke shadow".to_string(),
            move || Bounds::new(624.0, y0 + 624.0, 100.0, 16.0),
            || TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    View::group(children)
}

fn gradients_section(y0: f32) -> View {
    let mut children: Vec<View> = Vec::new();

    // separator + header
    children.push(
        Line::new(
            move || Point::new(24.0, y0),
            move || Point::new(760.0, y0),
            || LineStyle::new(CARD_BORDER, 1.0),
        )
        .view(),
    );
    children.push(
        Text::new(
            || "Gradients".to_string(),
            move || Bounds::new(24.0, y0 + 12.0, 200.0, 20.0),
            || TextStyle::new(12.0, MUTED),
        )
        .view(),
    );

    children.push(
        Text::new(
            || "Linear — Rect".to_string(),
            move || Bounds::new(24.0, y0 + 40.0, 300.0, 16.0),
            || TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    children.push(
        Rect::new(
            move || Bounds::new(24.0, y0 + 60.0, 168.0, 80.0),
            move || RectStyle {
                fill: Some(FillStyle::LinearGradient(LinearGradient::new(
                    Point::new(24.0, y0 + 100.0),
                    Point::new(192.0, y0 + 100.0),
                    &[
                        (0.0, Color::rgba(0.92, 0.27, 0.27, 1.0)),
                        (1.0, Color::rgba(0.24, 0.47, 0.98, 1.0)),
                    ],
                ))),
                stroke: None,
                radius: BorderRadius::all(8.0),
                shadow: None,
            },
        )
        .view(),
    );
    children.push(
        Text::new(
            || "horizontal".to_string(),
            move || Bounds::new(24.0, y0 + 146.0, 168.0, 16.0),
            || TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    children.push(
        Rect::new(
            move || Bounds::new(208.0, y0 + 60.0, 168.0, 80.0),
            move || RectStyle {
                fill: Some(FillStyle::LinearGradient(LinearGradient::new(
                    Point::new(292.0, y0 + 60.0),
                    Point::new(292.0, y0 + 140.0),
                    &[(0.0, PURPLE), (1.0, SUCCESS)],
                ))),
                stroke: None,
                radius: BorderRadius::all(8.0),
                shadow: None,
            },
        )
        .view(),
    );
    children.push(
        Text::new(
            || "vertical".to_string(),
            move || Bounds::new(208.0, y0 + 146.0, 168.0, 16.0),
            || TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    children.push(
        Rect::new(
            move || Bounds::new(392.0, y0 + 60.0, 168.0, 80.0),
            move || RectStyle {
                fill: Some(FillStyle::LinearGradient(LinearGradient::new(
                    Point::new(392.0, y0 + 60.0),
                    Point::new(560.0, y0 + 140.0),
                    &[(0.0, WARNING), (1.0, DARK)],
                ))),
                stroke: None,
                radius: BorderRadius::all(8.0),
                shadow: None,
            },
        )
        .view(),
    );
    children.push(
        Text::new(
            || "diagonal".to_string(),
            move || Bounds::new(392.0, y0 + 146.0, 168.0, 16.0),
            || TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    children.push(
        Rect::new(
            move || Bounds::new(576.0, y0 + 60.0, 168.0, 80.0),
            move || RectStyle {
                fill: Some(FillStyle::LinearGradient(LinearGradient::new(
                    Point::new(576.0, y0 + 100.0),
                    Point::new(744.0, y0 + 100.0),
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
        )
        .view(),
    );
    children.push(
        Text::new(
            || "3 stops".to_string(),
            move || Bounds::new(576.0, y0 + 146.0, 168.0, 16.0),
            || TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    children.push(
        Text::new(
            || "Radial — Rect".to_string(),
            move || Bounds::new(24.0, y0 + 180.0, 300.0, 16.0),
            || TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    children.push(
        Rect::new(
            move || Bounds::new(24.0, y0 + 200.0, 168.0, 80.0),
            move || RectStyle {
                fill: Some(FillStyle::RadialGradient(RadialGradient::new(
                    Point::new(108.0, y0 + 240.0),
                    70.0,
                    &[(0.0, PRIMARY), (1.0, Color::rgba(0.24, 0.47, 0.98, 0.0))],
                ))),
                stroke: None,
                radius: BorderRadius::all(8.0),
                shadow: None,
            },
        )
        .view(),
    );
    children.push(
        Text::new(
            || "center burst".to_string(),
            move || Bounds::new(24.0, y0 + 286.0, 168.0, 16.0),
            || TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    children.push(
        Rect::new(
            move || Bounds::new(208.0, y0 + 200.0, 168.0, 80.0),
            move || RectStyle {
                fill: Some(FillStyle::RadialGradient(RadialGradient::new(
                    Point::new(292.0, y0 + 240.0),
                    40.0,
                    &[(0.0, DANGER), (1.0, WARNING)],
                ))),
                stroke: None,
                radius: BorderRadius::all(8.0),
                shadow: None,
            },
        )
        .view(),
    );
    children.push(
        Text::new(
            || "tight radius".to_string(),
            move || Bounds::new(208.0, y0 + 286.0, 168.0, 16.0),
            || TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    children.push(
        Rect::new(
            move || Bounds::new(392.0, y0 + 200.0, 168.0, 80.0),
            move || RectStyle {
                fill: Some(FillStyle::RadialGradient(RadialGradient::new(
                    Point::new(476.0, y0 + 240.0),
                    80.0,
                    &[(0.0, WHITE), (0.45, PURPLE), (1.0, DARK)],
                ))),
                stroke: None,
                radius: BorderRadius::all(8.0),
                shadow: None,
            },
        )
        .view(),
    );
    children.push(
        Text::new(
            || "3 stops".to_string(),
            move || Bounds::new(392.0, y0 + 286.0, 168.0, 16.0),
            || TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    children.push(
        Rect::new(
            move || Bounds::new(576.0, y0 + 200.0, 168.0, 80.0),
            move || RectStyle {
                fill: Some(FillStyle::RadialGradient(RadialGradient::new(
                    Point::new(576.0, y0 + 200.0),
                    180.0,
                    &[(0.0, SUCCESS), (1.0, DARK)],
                ))),
                stroke: None,
                radius: BorderRadius::all(8.0),
                shadow: None,
            },
        )
        .view(),
    );
    children.push(
        Text::new(
            || "off-center".to_string(),
            move || Bounds::new(576.0, y0 + 286.0, 168.0, 16.0),
            || TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    children.push(
        Text::new(
            || "Gradients — Path".to_string(),
            move || Bounds::new(24.0, y0 + 318.0, 300.0, 16.0),
            || TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    let tri = Rc::new(
        PathData::new()
            .move_to(Point::new(99.0, y0 + 338.0))
            .line_to(Point::new(174.0, y0 + 468.0))
            .line_to(Point::new(24.0, y0 + 468.0))
            .close(),
    );
    children.push(
        Path::new(
            {
                let d = tri.clone();
                move || d.clone()
            },
            move || PathStyle {
                fill: Some(FillStyle::LinearGradient(LinearGradient::new(
                    Point::new(99.0, y0 + 338.0),
                    Point::new(99.0, y0 + 468.0),
                    &[(0.0, DANGER), (1.0, WARNING)],
                ))),
                stroke: None,
                fill_rule: FillRule::Winding,
                shadow: None,
            },
        )
        .view(),
    );
    children.push(
        Text::new(
            || "triangle linear".to_string(),
            move || Bounds::new(24.0, y0 + 476.0, 180.0, 16.0),
            || TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    let cx = 292.0f32;
    let cy = y0 + 403.0;
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
            move || PathStyle {
                fill: Some(FillStyle::RadialGradient(RadialGradient::new(
                    Point::new(cx, cy),
                    outer,
                    &[(0.0, WHITE), (0.5, PURPLE), (1.0, DARK)],
                ))),
                stroke: Some(Stroke::new(DARK, 1.0)),
                fill_rule: FillRule::Winding,
                shadow: None,
            },
        )
        .view(),
    );
    children.push(
        Text::new(
            || "star radial".to_string(),
            move || Bounds::new(224.0, y0 + 476.0, 180.0, 16.0),
            || TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    let petal = Rc::new(
        PathData::new()
            .move_to(Point::new(476.0, y0 + 338.0))
            .cubic_to(
                Point::new(556.0, y0 + 338.0),
                Point::new(556.0, y0 + 468.0),
                Point::new(476.0, y0 + 468.0),
            )
            .cubic_to(
                Point::new(396.0, y0 + 468.0),
                Point::new(396.0, y0 + 338.0),
                Point::new(476.0, y0 + 338.0),
            )
            .close(),
    );
    children.push(
        Path::new(
            {
                let d = petal.clone();
                move || d.clone()
            },
            move || PathStyle {
                fill: Some(FillStyle::LinearGradient(LinearGradient::new(
                    Point::new(396.0, y0 + 338.0),
                    Point::new(556.0, y0 + 468.0),
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
    children.push(
        Text::new(
            || "petal linear 3-stop".to_string(),
            move || Bounds::new(396.0, y0 + 476.0, 180.0, 16.0),
            || TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    let rings = Rc::new(
        PathData::new()
            .move_to(Point::new(600.0, y0 + 338.0))
            .line_to(Point::new(760.0, y0 + 338.0))
            .line_to(Point::new(760.0, y0 + 468.0))
            .line_to(Point::new(600.0, y0 + 468.0))
            .close()
            .move_to(Point::new(624.0, y0 + 362.0))
            .line_to(Point::new(736.0, y0 + 362.0))
            .line_to(Point::new(736.0, y0 + 444.0))
            .line_to(Point::new(624.0, y0 + 444.0))
            .close(),
    );
    children.push(
        Path::new(
            {
                let d = rings.clone();
                move || d.clone()
            },
            move || PathStyle {
                fill: Some(FillStyle::LinearGradient(LinearGradient::new(
                    Point::new(600.0, y0 + 403.0),
                    Point::new(760.0, y0 + 403.0),
                    &[(0.0, DANGER), (1.0, PURPLE)],
                ))),
                stroke: None,
                fill_rule: FillRule::EvenOdd,
                shadow: None,
            },
        )
        .view(),
    );
    children.push(
        Text::new(
            || "even-odd + linear".to_string(),
            move || Bounds::new(600.0, y0 + 476.0, 180.0, 16.0),
            || TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    View::group(children)
}

fn layers_section(y0: f32) -> View {
    let mut children: Vec<View> = Vec::new();

    children.push(
        Line::new(
            move || Point::new(24.0, y0),
            move || Point::new(760.0, y0),
            || LineStyle::new(CARD_BORDER, 1.0),
        )
        .view(),
    );
    children.push(
        Text::new(
            || "Layers (PushLayer / PopLayer)".to_string(),
            move || Bounds::new(24.0, y0 + 12.0, 400.0, 20.0),
            || TextStyle::new(12.0, MUTED),
        )
        .view(),
    );

    children.push(
        Text::new(
            || "Opacity — same red rect at 1.0 / 0.6 / 0.3 / 0.1".to_string(),
            move || Bounds::new(24.0, y0 + 40.0, 500.0, 16.0),
            || TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    for (i, &opacity) in [1.0f32, 0.6, 0.3, 0.1].iter().enumerate() {
        let x = 24.0 + i as f32 * 184.0;
        children.push(View::Layer {
            opacity,
            children: vec![
                Rect::new(
                    move || Bounds::new(x, y0 + 60.0, 168.0, 80.0),
                    || RectStyle {
                        fill: Some(FillStyle::Solid(DANGER)),
                        stroke: None,
                        radius: BorderRadius::all(8.0),
                        shadow: None,
                    },
                )
                .view(),
                Text::new(
                    move || format!("{opacity:.1}"),
                    move || Bounds::new(x, y0 + 64.0, 168.0, 72.0),
                    || TextStyle::new(18.0, WHITE),
                )
                .view(),
            ],
        });
    }

    children.push(
        Text::new(
            || "Overlapping colored layers at 0.7 opacity".to_string(),
            move || Bounds::new(24.0, y0 + 164.0, 500.0, 16.0),
            || TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    children.push(
        Rect::new(
            move || Bounds::new(24.0, y0 + 184.0, 368.0, 180.0),
            || RectStyle {
                fill: Some(FillStyle::Solid(DARK)),
                stroke: None,
                radius: BorderRadius::all(8.0),
                shadow: None,
            },
        )
        .view(),
    );

    children.push(View::Layer {
        opacity: 0.7,
        children: vec![
            Rect::new(
                move || Bounds::new(40.0, y0 + 200.0, 180.0, 120.0),
                || RectStyle {
                    fill: Some(FillStyle::Solid(PRIMARY)),
                    stroke: None,
                    radius: BorderRadius::all(8.0),
                    shadow: None,
                },
            )
            .view(),
        ],
    });

    children.push(View::Layer {
        opacity: 0.7,
        children: vec![
            Rect::new(
                move || Bounds::new(120.0, y0 + 240.0, 180.0, 120.0),
                || RectStyle {
                    fill: Some(FillStyle::Solid(SUCCESS)),
                    stroke: None,
                    radius: BorderRadius::all(8.0),
                    shadow: None,
                },
            )
            .view(),
        ],
    });

    children.push(View::Layer {
        opacity: 0.7,
        children: vec![
            Rect::new(
                move || Bounds::new(200.0, y0 + 220.0, 180.0, 120.0),
                || RectStyle {
                    fill: Some(FillStyle::Solid(DANGER)),
                    stroke: None,
                    radius: BorderRadius::all(8.0),
                    shadow: None,
                },
            )
            .view(),
        ],
    });

    children.push(
        Text::new(
            || "Layer (0.8) wrapping a gradient rect + text".to_string(),
            move || Bounds::new(420.0, y0 + 164.0, 360.0, 16.0),
            || TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    children.push(View::Layer {
        opacity: 0.8,
        children: vec![
            Rect::new(
                move || Bounds::new(420.0, y0 + 184.0, 320.0, 180.0),
                move || RectStyle {
                    fill: Some(FillStyle::LinearGradient(LinearGradient::new(
                        Point::new(420.0, y0 + 274.0),
                        Point::new(740.0, y0 + 274.0),
                        &[(0.0, PRIMARY), (0.5, PURPLE), (1.0, DANGER)],
                    ))),
                    stroke: None,
                    radius: BorderRadius::all(12.0),
                    shadow: None,
                },
            )
            .view(),
            Text::new(
                || "gradient + layer".to_string(),
                move || Bounds::new(420.0, y0 + 254.0, 320.0, 60.0),
                || TextStyle::new(18.0, WHITE),
            )
            .view(),
        ],
    });

    children.push(
        Text::new(
            || "Nested layers: outer 0.6, inner 0.5 → combined ~0.3".to_string(),
            move || Bounds::new(24.0, y0 + 390.0, 500.0, 16.0),
            || TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    children.push(View::Layer {
        opacity: 0.6,
        children: vec![
            Rect::new(
                move || Bounds::new(24.0, y0 + 410.0, 340.0, 120.0),
                || RectStyle {
                    fill: Some(FillStyle::Solid(PRIMARY)),
                    stroke: None,
                    radius: BorderRadius::all(8.0),
                    shadow: None,
                },
            )
            .view(),
            View::Layer {
                opacity: 0.5,
                children: vec![
                    Rect::new(
                        move || Bounds::new(60.0, y0 + 430.0, 260.0, 80.0),
                        || RectStyle {
                            fill: Some(FillStyle::Solid(DANGER)),
                            stroke: None,
                            radius: BorderRadius::all(6.0),
                            shadow: None,
                        },
                    )
                    .view(),
                    Text::new(
                        || "inner 0.5".to_string(),
                        move || Bounds::new(60.0, y0 + 434.0, 260.0, 72.0),
                        || TextStyle::new(14.0, WHITE),
                    )
                    .view(),
                ],
            },
            Text::new(
                || "outer 0.6".to_string(),
                move || Bounds::new(24.0, y0 + 414.0, 340.0, 20.0),
                || TextStyle::new(11.0, Color::rgba(1.0, 1.0, 1.0, 0.7)),
            )
            .view(),
        ],
    });

    View::group(children)
}

fn shadows_section(y0: f32) -> View {
    let mut children: Vec<View> = Vec::new();

    children.push(
        Line::new(
            move || Point::new(24.0, y0),
            move || Point::new(760.0, y0),
            || LineStyle::new(CARD_BORDER, 1.0),
        )
        .view(),
    );
    children.push(
        Text::new(
            || "Shadows".to_string(),
            move || Bounds::new(24.0, y0 + 12.0, 300.0, 20.0),
            || TextStyle::new(12.0, MUTED),
        )
        .view(),
    );

    children.push(
        Text::new(
            || "Rect shadows — offset / blur / spread".to_string(),
            move || Bounds::new(24.0, y0 + 40.0, 400.0, 16.0),
            || TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    children.push(
        Rect::new(
            move || Bounds::new(24.0, y0 + 60.0, 152.0, 80.0),
            || RectStyle {
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
        )
        .view(),
    );
    children.push(
        Text::new(
            || "soft (0, 4, 12)".to_string(),
            move || Bounds::new(24.0, y0 + 146.0, 152.0, 16.0),
            || TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    children.push(
        Rect::new(
            move || Bounds::new(200.0, y0 + 60.0, 152.0, 80.0),
            || RectStyle {
                fill: Some(FillStyle::Solid(WHITE)),
                stroke: None,
                radius: BorderRadius::all(8.0),
                shadow: Some(Shadow::new(4.0, 8.0, 4.0, Color::rgba(0.0, 0.0, 0.0, 0.4))),
            },
        )
        .view(),
    );
    children.push(
        Text::new(
            || "offset (4, 8, 4)".to_string(),
            move || Bounds::new(200.0, y0 + 146.0, 152.0, 16.0),
            || TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    children.push(
        Rect::new(
            move || Bounds::new(376.0, y0 + 60.0, 152.0, 80.0),
            || RectStyle {
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
        )
        .view(),
    );
    children.push(
        Text::new(
            || "colored primary".to_string(),
            move || Bounds::new(376.0, y0 + 146.0, 152.0, 16.0),
            || TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    children.push(
        Rect::new(
            move || Bounds::new(552.0, y0 + 60.0, 152.0, 80.0),
            || RectStyle {
                fill: Some(FillStyle::Solid(WHITE)),
                stroke: None,
                radius: BorderRadius::all(8.0),
                shadow: Some(
                    Shadow::new(0.0, 0.0, 8.0, Color::rgba(0.0, 0.0, 0.0, 0.3)).with_spread(4.0),
                ),
            },
        )
        .view(),
    );
    children.push(
        Text::new(
            || "spread +4".to_string(),
            move || Bounds::new(552.0, y0 + 146.0, 152.0, 16.0),
            || TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    children.push(
        Text::new(
            || "Colored shadows on dark cards".to_string(),
            move || Bounds::new(24.0, y0 + 176.0, 400.0, 16.0),
            || TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    let card_colors: &[(Color, Color, &str)] = &[
        (PRIMARY, Color::rgba(0.24, 0.47, 0.98, 0.6), "primary glow"),
        (SUCCESS, Color::rgba(0.18, 0.69, 0.45, 0.6), "success glow"),
        (DANGER, Color::rgba(0.92, 0.27, 0.27, 0.6), "danger glow"),
        (PURPLE, Color::rgba(0.60, 0.28, 0.98, 0.6), "purple glow"),
    ];
    for (i, &(card_color, shadow_color, label)) in card_colors.iter().enumerate() {
        let x = 24.0 + i as f32 * 184.0;
        children.push(
            Rect::new(
                move || Bounds::new(x, y0 + 196.0, 168.0, 80.0),
                move || RectStyle {
                    fill: Some(FillStyle::Solid(card_color)),
                    stroke: None,
                    radius: BorderRadius::all(10.0),
                    shadow: Some(Shadow::new(0.0, 8.0, 20.0, shadow_color)),
                },
            )
            .view(),
        );
        children.push(
            Text::new(
                || label.to_string(),
                move || Bounds::new(x, y0 + 200.0, 168.0, 72.0),
                || TextStyle::new(12.0, WHITE),
            )
            .view(),
        );
    }

    children.push(
        Text::new(
            || "Text shadows".to_string(),
            move || Bounds::new(24.0, y0 + 312.0, 300.0, 16.0),
            || TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    children.push(
        Rect::new(
            move || Bounds::new(24.0, y0 + 332.0, 720.0, 100.0),
            || RectStyle {
                fill: Some(FillStyle::Solid(WHITE)),
                stroke: Some(Stroke::new(CARD_BORDER, 1.0)),
                radius: BorderRadius::all(8.0),
                shadow: None,
            },
        )
        .view(),
    );

    children.push(
        Text::new(
            || "Drop shadow".to_string(),
            move || Bounds::new(40.0, y0 + 348.0, 180.0, 30.0),
            || {
                TextStyle::new(22.0, DARK).with_shadow(Shadow::new(
                    2.0,
                    3.0,
                    5.0,
                    Color::rgba(0.0, 0.0, 0.0, 0.35),
                ))
            },
        )
        .view(),
    );

    children.push(
        Text::new(
            || "Color glow".to_string(),
            move || Bounds::new(240.0, y0 + 348.0, 200.0, 30.0),
            || {
                TextStyle::new(22.0, PRIMARY).with_shadow(Shadow::new(
                    0.0,
                    0.0,
                    8.0,
                    Color::rgba(0.24, 0.47, 0.98, 0.6),
                ))
            },
        )
        .view(),
    );

    children.push(
        Text::new(
            || "Hard offset".to_string(),
            move || Bounds::new(460.0, y0 + 348.0, 200.0, 30.0),
            || {
                TextStyle::new(22.0, DARK).with_shadow(Shadow::new(
                    3.0,
                    3.0,
                    1.0,
                    Color::rgba(0.92, 0.27, 0.27, 0.7),
                ))
            },
        )
        .view(),
    );

    children.push(
        Text::new(
            || "Shadow inside layer".to_string(),
            move || Bounds::new(24.0, y0 + 452.0, 400.0, 16.0),
            || TextStyle::new(11.0, MUTED),
        )
        .view(),
    );

    children.push(View::Layer {
        opacity: 1.0,
        children: vec![
            Rect::new(
                move || Bounds::new(24.0, y0 + 472.0, 220.0, 100.0),
                || RectStyle {
                    fill: Some(FillStyle::Solid(WHITE)),
                    stroke: None,
                    radius: BorderRadius::all(10.0),
                    shadow: Some(Shadow::new(0.0, 6.0, 16.0, Color::rgba(0.0, 0.0, 0.0, 0.2))),
                },
            )
            .view(),
            Text::new(
                || "layer opacity 1.0".to_string(),
                move || Bounds::new(40.0, y0 + 500.0, 200.0, 20.0),
                || TextStyle::new(12.0, DARK),
            )
            .view(),
        ],
    });

    children.push(View::Layer {
        opacity: 0.7,
        children: vec![
            Rect::new(
                move || Bounds::new(264.0, y0 + 472.0, 220.0, 100.0),
                || RectStyle {
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
            )
            .view(),
            Text::new(
                || "layer opacity 0.7".to_string(),
                move || Bounds::new(280.0, y0 + 500.0, 200.0, 20.0),
                || TextStyle::new(12.0, DARK),
            )
            .view(),
        ],
    });

    children.push(View::Layer {
        opacity: 0.4,
        children: vec![
            Rect::new(
                move || Bounds::new(504.0, y0 + 472.0, 220.0, 100.0),
                || RectStyle {
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
            )
            .view(),
            Text::new(
                || "layer opacity 0.4".to_string(),
                move || Bounds::new(520.0, y0 + 500.0, 200.0, 20.0),
                || TextStyle::new(12.0, DARK),
            )
            .view(),
        ],
    });

    View::group(children)
}

struct ScrollableContent {
    gradient: Rc<ImageData>,
    checker: Rc<ImageData>,
    alpha: Rc<ImageData>,
}

impl Component for ScrollableContent {
    fn view(&self) -> View {
        static_content(
            self.gradient.clone(),
            self.checker.clone(),
            self.alpha.clone(),
        )
    }

    fn on_event(&mut self, _: &Event) -> EventResult {
        EventResult::Ignored
    }
}

struct SandboxRootComponent {
    window_width: RwSignal<f32>,
    window_height: RwSignal<f32>,
    scroll_area: ScrollArea,
    widget_panel: TranslateGroup,
}

impl Component for SandboxRootComponent {
    fn view(&self) -> View {
        let widget_label = Text::new(
            || "Reactive Widgets".to_string(),
            || Bounds::new(PANEL_X, PANEL_Y - 18.0, PANEL_W, 14.0),
            || TextStyle::new(11.0, MUTED),
        )
        .view();
        let panel_bg = Rect::new(
            || Bounds::new(PANEL_X, PANEL_Y, PANEL_W, PANEL_H),
            || RectStyle {
                fill: Some(FillStyle::Solid(DARK)),
                stroke: Some(Stroke::new(CARD_BORDER, 1.0)),
                radius: BorderRadius::all(8.0),
                shadow: None,
            },
        )
        .view();

        View::Group(vec![
            widget_label,
            self.scroll_area.view(),
            panel_bg,
            self.widget_panel.view(),
        ])
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        let handled = self
            .widget_panel
            .on_event(event)
            .or(self.scroll_area.on_event(event));

        if let Event::WindowResized { width, height } = event {
            self.window_width.set(*width as f32);
            self.window_height.set(*height as f32);
            return EventResult::Handled;
        }

        handled
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

        let count = create_rw_signal(0i32);

        let ((count_label, btn_inc, btn_dec), _) = with_context(WidgetCtx::new(), || {
            let c = count.clone();
            let count_label = Label::new(
                move || format!("Count: {}", c.get()),
                LayoutStyle::new().width(PANEL_W - 16.0).height(24.0),
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

        let ww = window_width.clone();
        let wh = window_height.clone();
        let scroll_area = ScrollArea::new(
            move || Bounds::new(0.0, 0.0, ww.get(), wh.get()),
            CONTENT_HEIGHT,
            vec![Box::new(ScrollableContent {
                gradient: gradient_image,
                checker: checker_image,
                alpha: alpha_image,
            })],
        );

        Box::new(SandboxRootComponent {
            window_width,
            window_height,
            scroll_area,
            widget_panel: TranslateGroup::new(
                || PANEL_X,
                || PANEL_Y,
                vec![Box::new(WidgetPanel {
                    count_label,
                    btn_inc,
                    btn_dec,
                })],
            ),
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
