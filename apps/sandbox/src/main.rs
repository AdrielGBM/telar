use std::sync::Arc;

use rsx::{
    App, AppContext, BorderRadius, Color, FillStyle, Frame, ImageData, ImageFilter, Rect, Stroke,
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

struct Sandbox {
    gradient_image: Arc<ImageData>,
    checker_image: Arc<ImageData>,
    alpha_image: Arc<ImageData>,
}

impl App for Sandbox {
    fn on_redraw(&mut self, frame: &mut Frame, _ctx: &mut AppContext) {
        frame.clear(SURFACE);

        frame.draw_text(
            "Shapes",
            Rect::new(24.0, 20.0, 200.0, 20.0),
            TextStyle::new(12.0, MUTED),
        );

        frame.draw_rect(
            Rect::new(24.0, 44.0, 168.0, 80.0),
            Some(FillStyle::solid(PRIMARY)),
            None,
            BorderRadius::all(8.0),
        );
        frame.draw_text(
            "fill",
            Rect::new(24.0, 48.0, 168.0, 72.0),
            TextStyle::new(13.0, WHITE),
        );

        frame.draw_rect(
            Rect::new(208.0, 44.0, 168.0, 80.0),
            None,
            Some(Stroke::new(DANGER, 2.0)),
            BorderRadius::all(8.0),
        );
        frame.draw_text(
            "stroke",
            Rect::new(208.0, 48.0, 168.0, 72.0),
            TextStyle::new(13.0, DANGER),
        );

        frame.draw_rect(
            Rect::new(392.0, 44.0, 168.0, 80.0),
            Some(FillStyle::solid(SUCCESS)),
            Some(Stroke::new(DARK, 1.5)),
            BorderRadius::zero(),
        );
        frame.draw_text(
            "fill + stroke",
            Rect::new(392.0, 48.0, 168.0, 72.0),
            TextStyle::new(13.0, WHITE),
        );

        frame.draw_rect(
            Rect::new(576.0, 44.0, 168.0, 80.0),
            Some(FillStyle::solid(PURPLE)),
            None,
            BorderRadius::all(40.0),
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
                Some(FillStyle::solid(color)),
                None,
                BorderRadius::all(6.0),
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
            Some(FillStyle::solid(DARK)),
            None,
            BorderRadius::all(10.0),
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
            Some(FillStyle::solid(WHITE)),
            Some(Stroke::new(CARD_BORDER, 1.0)),
            BorderRadius::all(10.0),
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
    }
}

fn main() {
    let gradient_image = Arc::new(make_gradient(128, 128));
    let checker_image = Arc::new(make_checker(128, 128, 16));
    let alpha_image = Arc::new(make_radial_alpha(128, 128));
    rsx::run!(
        WindowConfig::default(),
        Sandbox {
            gradient_image,
            checker_image,
            alpha_image,
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
