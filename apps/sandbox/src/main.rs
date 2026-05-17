use std::sync::Arc;

use rsx::{
    App, AppContext, BorderRadius, Color, Event, FillRule, FillStyle, Frame, ImageData,
    ImageFilter, LineCap, LineJoin, LineStyle, PathData, PathStyle, Point, Rect, RectStyle, Stroke,
    TextStyle, WindowConfig,
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

struct Sandbox {
    gradient_image: Arc<ImageData>,
    checker_image: Arc<ImageData>,
    alpha_image: Arc<ImageData>,
    scroll_y: f32,
    window_width: f32,
    window_height: f32,
}

impl App for Sandbox {
    fn on_event(&mut self, event: Event, ctx: &mut AppContext) {
        if let Event::Scrolled { delta_y, .. } = event {
            let max_scroll = (CONTENT_HEIGHT - self.window_height).max(0.0);
            self.scroll_y = (self.scroll_y - delta_y as f32).clamp(0.0, max_scroll);
            ctx.request_redraw();
        }
        if let Event::WindowResized { width, height } = event {
            self.window_width = width as f32;
            self.window_height = height as f32;
            ctx.request_redraw();
        }
    }

    fn on_redraw(&mut self, frame: &mut Frame, _ctx: &mut AppContext) {
        frame.clear(SURFACE);
        frame.push_clip(Rect::new(0.0, 0.0, f32::MAX, self.window_height));
        frame.push_translate(0.0, -self.scroll_y);

        frame.draw_text(
            "Shapes",
            Rect::new(24.0, 20.0, 200.0, 20.0),
            TextStyle::new(12.0, MUTED),
        );

        frame.draw_rect(
            Rect::new(24.0, 44.0, 168.0, 80.0),
            RectStyle {
                fill: Some(FillStyle::solid(PRIMARY)),
                stroke: None,
                radius: BorderRadius::all(8.0),
            },
        );
        frame.draw_text(
            "fill",
            Rect::new(24.0, 48.0, 168.0, 72.0),
            TextStyle::new(13.0, WHITE),
        );

        frame.draw_rect(
            Rect::new(208.0, 44.0, 168.0, 80.0),
            RectStyle {
                fill: None,
                stroke: Some(Stroke::new(DANGER, 2.0)),
                radius: BorderRadius::all(8.0),
            },
        );
        frame.draw_text(
            "stroke",
            Rect::new(208.0, 48.0, 168.0, 72.0),
            TextStyle::new(13.0, DANGER),
        );

        frame.draw_rect(
            Rect::new(392.0, 44.0, 168.0, 80.0),
            RectStyle {
                fill: Some(FillStyle::solid(SUCCESS)),
                stroke: Some(Stroke::new(DARK, 1.5)),
                radius: BorderRadius::zero(),
            },
        );
        frame.draw_text(
            "fill + stroke",
            Rect::new(392.0, 48.0, 168.0, 72.0),
            TextStyle::new(13.0, WHITE),
        );

        frame.draw_rect(
            Rect::new(576.0, 44.0, 168.0, 80.0),
            RectStyle {
                fill: Some(FillStyle::solid(PURPLE)),
                stroke: None,
                radius: BorderRadius::all(40.0),
            },
        );
        frame.draw_text(
            "pill radius",
            Rect::new(576.0, 48.0, 168.0, 72.0),
            TextStyle::new(13.0, WHITE),
        );

        frame.draw_text(
            "Colors",
            Rect::new(24.0, 148.0, 200.0, 20.0),
            TextStyle::new(12.0, MUTED),
        );

        let swatches = [PRIMARY, SUCCESS, DANGER, WARNING, PURPLE, DARK];
        let labels = ["primary", "success", "danger", "warning", "purple", "dark"];
        for (i, (&color, &label)) in swatches.iter().zip(labels.iter()).enumerate() {
            let x = 24.0 + i as f32 * 116.0;
            frame.draw_rect(
                Rect::new(x, 172.0, 100.0, 44.0),
                RectStyle {
                    fill: Some(FillStyle::solid(color)),
                    stroke: None,
                    radius: BorderRadius::all(6.0),
                },
            );
            frame.draw_text(
                label,
                Rect::new(x, 176.0, 100.0, 36.0),
                TextStyle::new(11.0, WHITE),
            );
        }

        frame.draw_text(
            "Typography",
            Rect::new(24.0, 240.0, 300.0, 20.0),
            TextStyle::new(12.0, MUTED),
        );

        frame.draw_text(
            "Small — 12px — The quick brown fox",
            Rect::new(24.0, 262.0, 600.0, 20.0),
            TextStyle::new(12.0, DARK),
        );
        frame.draw_text(
            "Regular — 14px — The quick brown fox",
            Rect::new(24.0, 286.0, 600.0, 22.0),
            TextStyle::new(14.0, DARK),
        );
        frame.draw_text(
            "Medium — 18px — The quick brown fox",
            Rect::new(24.0, 312.0, 600.0, 26.0),
            TextStyle::new(18.0, DARK),
        );
        frame.draw_text(
            "Large — 24px — The quick brown fox",
            Rect::new(24.0, 342.0, 700.0, 32.0),
            TextStyle::new(24.0, DARK),
        );
        frame.draw_text(
            "Display — 32px",
            Rect::new(24.0, 378.0, 500.0, 42.0),
            TextStyle::new(32.0, PRIMARY),
        );

        frame.draw_text(
            "Cards",
            Rect::new(24.0, 440.0, 200.0, 20.0),
            TextStyle::new(12.0, MUTED),
        );

        frame.draw_rect(
            Rect::new(24.0, 464.0, 368.0, 110.0),
            RectStyle {
                fill: Some(FillStyle::solid(DARK)),
                stroke: None,
                radius: BorderRadius::all(10.0),
            },
        );
        frame.draw_text(
            "Dark Card",
            Rect::new(40.0, 478.0, 340.0, 24.0),
            TextStyle::new(16.0, WHITE),
        );
        frame.draw_text(
            "White text on a dark background.",
            Rect::new(40.0, 508.0, 340.0, 52.0),
            TextStyle::new(13.0, MUTED),
        );

        frame.draw_rect(
            Rect::new(408.0, 464.0, 368.0, 110.0),
            RectStyle {
                fill: Some(FillStyle::solid(WHITE)),
                stroke: Some(Stroke::new(CARD_BORDER, 1.0)),
                radius: BorderRadius::all(10.0),
            },
        );
        frame.draw_text(
            "Light Card",
            Rect::new(424.0, 478.0, 340.0, 24.0),
            TextStyle::new(16.0, DARK),
        );
        frame.draw_text(
            "Dark text on a white background.",
            Rect::new(424.0, 508.0, 340.0, 52.0),
            TextStyle::new(13.0, MUTED),
        );

        frame.draw_text(
            "Images",
            Rect::new(24.0, 600.0, 200.0, 20.0),
            TextStyle::new(12.0, MUTED),
        );

        frame.draw_image(
            self.gradient_image.clone(),
            Rect::new(24.0, 624.0, 128.0, 128.0),
            ImageFilter::Linear,
        );
        frame.draw_text(
            "gradient",
            Rect::new(24.0, 756.0, 128.0, 16.0),
            TextStyle::new(11.0, MUTED),
        );

        frame.draw_image(
            self.checker_image.clone(),
            Rect::new(172.0, 624.0, 192.0, 192.0),
            ImageFilter::Nearest,
        );
        frame.draw_text(
            "checker (scaled)",
            Rect::new(172.0, 820.0, 192.0, 16.0),
            TextStyle::new(11.0, MUTED),
        );

        frame.draw_image(
            self.alpha_image.clone(),
            Rect::new(384.0, 624.0, 128.0, 128.0),
            ImageFilter::Nearest,
        );
        frame.draw_text(
            "alpha blend",
            Rect::new(384.0, 756.0, 128.0, 16.0),
            TextStyle::new(11.0, MUTED),
        );

        frame.draw_text(
            "Lines",
            Rect::new(24.0, 860.0, 200.0, 20.0),
            TextStyle::new(12.0, MUTED),
        );

        frame.draw_text(
            "Width",
            Rect::new(24.0, 884.0, 60.0, 16.0),
            TextStyle::new(11.0, MUTED),
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
            frame.draw_text(
                label,
                Rect::new(24.0, cy - 8.0, 56.0, 16.0),
                TextStyle::new(11.0, MUTED),
            );
            frame.draw_line(
                Point::new(88.0, cy),
                Point::new(360.0, cy),
                LineStyle::new(PRIMARY, w),
            );
            cy += w.max(2.0) + 18.0;
        }

        frame.draw_text(
            "Color",
            Rect::new(420.0, 884.0, 60.0, 16.0),
            TextStyle::new(11.0, MUTED),
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
            frame.draw_line(
                Point::new(420.0, y),
                Point::new(680.0, y),
                LineStyle::new(color, 3.0),
            );
            frame.draw_text(
                label,
                Rect::new(688.0, y - 8.0, 80.0, 16.0),
                TextStyle::new(11.0, color),
            );
        }

        frame.draw_text(
            "Separator & chart",
            Rect::new(24.0, 1020.0, 300.0, 16.0),
            TextStyle::new(11.0, MUTED),
        );
        frame.draw_line(
            Point::new(24.0, 1040.0),
            Point::new(760.0, 1040.0),
            LineStyle::new(CARD_BORDER, 1.0),
        );

        let ax = 60.0f32;
        let cb = 1150.0f32;
        let ct = 1060.0f32;
        let ax_right = 400.0f32;
        frame.draw_line(
            Point::new(ax, ct),
            Point::new(ax, cb),
            LineStyle::new(MUTED, 1.0),
        );
        frame.draw_line(
            Point::new(ax, cb),
            Point::new(ax_right, cb),
            LineStyle::new(MUTED, 1.0),
        );

        let data_x = [ax, ax + 85.0, ax + 170.0, ax + 255.0, ax_right];
        let s1 = [1140.0f32, 1115.0, 1098.0, 1078.0, 1068.0];
        let s2 = [1135.0f32, 1112.0, 1100.0, 1088.0, 1075.0];
        let s3 = [1115.0f32, 1122.0, 1130.0, 1138.0, 1145.0];
        for i in 0..4 {
            frame.draw_line(
                Point::new(data_x[i], s1[i]),
                Point::new(data_x[i + 1], s1[i + 1]),
                LineStyle::new(PRIMARY, 2.0),
            );
            frame.draw_line(
                Point::new(data_x[i], s2[i]),
                Point::new(data_x[i + 1], s2[i + 1]),
                LineStyle::new(SUCCESS, 2.0),
            );
            frame.draw_line(
                Point::new(data_x[i], s3[i]),
                Point::new(data_x[i + 1], s3[i + 1]),
                LineStyle::new(DANGER, 2.0),
            );
        }

        frame.draw_text(
            "Diagonals",
            Rect::new(460.0, 1044.0, 120.0, 16.0),
            TextStyle::new(11.0, MUTED),
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
            frame.draw_line(
                Point::new(fan_cx, fan_cy),
                Point::new(tx, ty),
                LineStyle::new(color, 2.0).with_cap(LineCap::Round),
            );
        }

        draw_paths_section(frame);

        frame.pop_transform();
        frame.pop_clip();

        if CONTENT_HEIGHT > self.window_height {
            let bar_h = (self.window_height / CONTENT_HEIGHT * self.window_height).max(24.0);
            let max_scroll = (CONTENT_HEIGHT - self.window_height).max(1.0);
            let bar_y = (self.scroll_y / max_scroll) * (self.window_height - bar_h);
            frame.draw_rect(
                Rect::new(self.window_width - 8.0, bar_y, 6.0, bar_h),
                RectStyle {
                    fill: Some(FillStyle::solid(MUTED)),
                    stroke: None,
                    radius: BorderRadius::all(3.0),
                },
            );
        }
    }
}

fn main() {
    let gradient_image = Arc::new(make_gradient(128, 128));
    let checker_image = Arc::new(make_checker(128, 128, 16));
    let alpha_image = Arc::new(make_radial_alpha(128, 128));
    let config = WindowConfig::default();
    rsx::run!(
        config,
        Sandbox {
            gradient_image,
            checker_image,
            alpha_image,
            scroll_y: 0.0,
            window_width: 800.0,
            window_height: 600.0,
        }
    );
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

fn draw_paths_section(frame: &mut Frame) {
    use std::sync::Arc;

    const Y0: f32 = 1200.0;

    frame.draw_line(
        Point::new(24.0, Y0),
        Point::new(760.0, Y0),
        LineStyle::new(CARD_BORDER, 1.0),
    );
    frame.draw_text(
        "Paths",
        Rect::new(24.0, Y0 + 12.0, 200.0, 20.0),
        TextStyle::new(12.0, MUTED),
    );
    frame.draw_text(
        "Polygon shapes",
        Rect::new(24.0, Y0 + 36.0, 300.0, 16.0),
        TextStyle::new(11.0, MUTED),
    );

    frame.draw_path(
        Arc::new(
            PathData::new()
                .move_to(Point::new(99.0, Y0 + 56.0))
                .line_to(Point::new(159.0, Y0 + 166.0))
                .line_to(Point::new(39.0, Y0 + 166.0))
                .close(),
        ),
        PathStyle {
            fill: Some(FillStyle::solid(PRIMARY)),
            stroke: None,
            fill_rule: FillRule::Winding,
        },
    );
    frame.draw_text(
        "triangle",
        Rect::new(24.0, Y0 + 176.0, 150.0, 16.0),
        TextStyle::new(11.0, MUTED),
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
        frame.draw_path(
            Arc::new(path),
            PathStyle {
                fill: Some(FillStyle::solid(DANGER)),
                stroke: Some(Stroke::new(DARK, 1.0)),
                fill_rule: FillRule::Winding,
            },
        );
        frame.draw_text(
            "star (fill + stroke)",
            Rect::new(199.0, Y0 + 176.0, 200.0, 16.0),
            TextStyle::new(11.0, MUTED),
        );
    }

    frame.draw_path(
        Arc::new(
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
            fill: Some(FillStyle::solid(PURPLE)),
            stroke: None,
            fill_rule: FillRule::EvenOdd,
        },
    );
    frame.draw_text(
        "even-odd fill",
        Rect::new(374.0, Y0 + 176.0, 200.0, 16.0),
        TextStyle::new(11.0, MUTED),
    );

    frame.draw_text(
        "Bézier curves",
        Rect::new(24.0, Y0 + 212.0, 300.0, 16.0),
        TextStyle::new(11.0, MUTED),
    );

    frame.draw_path(
        Arc::new(
            PathData::new()
                .move_to(Point::new(24.0, Y0 + 308.0))
                .quad_to(Point::new(164.0, Y0 + 238.0), Point::new(304.0, Y0 + 308.0)),
        ),
        PathStyle {
            fill: None,
            stroke: Some(Stroke::new(WARNING, 3.0).with_cap(LineCap::Round)),
            fill_rule: FillRule::Winding,
        },
    );
    frame.draw_text(
        "quad_to arch",
        Rect::new(24.0, Y0 + 318.0, 200.0, 16.0),
        TextStyle::new(11.0, MUTED),
    );

    frame.draw_path(
        Arc::new(
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
    );
    frame.draw_text(
        "cubic_to S-curve",
        Rect::new(320.0, Y0 + 318.0, 200.0, 16.0),
        TextStyle::new(11.0, MUTED),
    );

    frame.draw_path(
        Arc::new(
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
            fill: Some(FillStyle::solid(Color::rgba(0.97, 0.72, 0.18, 0.75))),
            stroke: Some(Stroke::new(WARNING, 1.5)),
            fill_rule: FillRule::Winding,
        },
    );
    frame.draw_text(
        "closed cubic (petal)",
        Rect::new(470.0, Y0 + 318.0, 200.0, 16.0),
        TextStyle::new(11.0, MUTED),
    );

    frame.draw_text(
        "Stroke style",
        Rect::new(24.0, Y0 + 354.0, 300.0, 16.0),
        TextStyle::new(11.0, MUTED),
    );

    frame.draw_path(
        Arc::new(
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
    );
    frame.draw_text(
        "Butt / Miter (default)",
        Rect::new(24.0, Y0 + 448.0, 230.0, 16.0),
        TextStyle::new(11.0, MUTED),
    );

    frame.draw_path(
        Arc::new(
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
    );
    frame.draw_text(
        "Round cap / Round join",
        Rect::new(324.0, Y0 + 448.0, 240.0, 16.0),
        TextStyle::new(11.0, MUTED),
    );
}
