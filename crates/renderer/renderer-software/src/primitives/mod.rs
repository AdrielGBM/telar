pub(crate) mod image;
pub(crate) mod line;
pub(crate) mod path;
pub(crate) mod rect;
pub(crate) mod text;

pub(crate) fn to_skia_color(color: renderer_core::Color) -> tiny_skia::Color {
    tiny_skia::Color::from_rgba(
        color.r.clamp(0.0, 1.0),
        color.g.clamp(0.0, 1.0),
        color.b.clamp(0.0, 1.0),
        color.a.clamp(0.0, 1.0),
    )
    .expect("channels clamped to [0,1]")
}

pub(crate) fn fill_to_paint(
    fill: renderer_core::FillStyle,
    _transform: tiny_skia::Transform,
) -> tiny_skia::Paint<'static> {
    let mut paint = tiny_skia::Paint::default();
    paint.anti_alias = true;
    match fill {
        renderer_core::FillStyle::Solid(c) => {
            paint.set_color(to_skia_color(c));
        }
        renderer_core::FillStyle::LinearGradient(g) => {
            let stops: Vec<tiny_skia::GradientStop> = g.stops[..g.stop_count as usize]
                .iter()
                .map(|s| tiny_skia::GradientStop::new(s.position, to_skia_color(s.color)))
                .collect();
            if let Some(shader) = tiny_skia::LinearGradient::new(
                tiny_skia::Point::from_xy(g.start.x, g.start.y),
                tiny_skia::Point::from_xy(g.end.x, g.end.y),
                stops,
                tiny_skia::SpreadMode::Pad,
                tiny_skia::Transform::identity(),
            ) {
                paint.shader = shader;
            }
        }
        renderer_core::FillStyle::RadialGradient(g) => {
            let stops: Vec<tiny_skia::GradientStop> = g.stops[..g.stop_count as usize]
                .iter()
                .map(|s| tiny_skia::GradientStop::new(s.position, to_skia_color(s.color)))
                .collect();
            if let Some(shader) = tiny_skia::RadialGradient::new(
                tiny_skia::Point::from_xy(g.center.x, g.center.y),
                0.0,
                tiny_skia::Point::from_xy(g.center.x, g.center.y),
                g.radius,
                stops,
                tiny_skia::SpreadMode::Pad,
                tiny_skia::Transform::identity(),
            ) {
                paint.shader = shader;
            }
        }
    }
    paint
}

pub(crate) fn gaussian_blur(data: &mut [u8], width: u32, height: u32, sigma: f32) {
    if sigma < 0.5 || width == 0 || height == 0 {
        return;
    }
    let r = ((sigma * 1.5).round() as u32).max(1);
    for _ in 0..3 {
        box_blur_h(data, width, height, r);
        box_blur_v(data, width, height, r);
    }
}

fn box_blur_h(data: &mut [u8], width: u32, height: u32, r: u32) {
    let w = width as usize;
    let h = height as usize;
    let r = r as usize;
    let mut tmp = data.to_vec();
    for y in 0..h {
        for x in 0..w {
            let x0 = x.saturating_sub(r);
            let x1 = (x + r + 1).min(w);
            let n = (x1 - x0) as u32;
            for c in 0..4usize {
                let sum: u32 = (x0..x1).map(|xi| data[(y * w + xi) * 4 + c] as u32).sum();
                tmp[(y * w + x) * 4 + c] = (sum / n) as u8;
            }
        }
    }
    data.copy_from_slice(&tmp);
}

fn box_blur_v(data: &mut [u8], width: u32, height: u32, r: u32) {
    let w = width as usize;
    let h = height as usize;
    let r = r as usize;
    let mut tmp = data.to_vec();
    for y in 0..h {
        for x in 0..w {
            let y0 = y.saturating_sub(r);
            let y1 = (y + r + 1).min(h);
            let n = (y1 - y0) as u32;
            for c in 0..4usize {
                let sum: u32 = (y0..y1).map(|yi| data[(yi * w + x) * 4 + c] as u32).sum();
                tmp[(y * w + x) * 4 + c] = (sum / n) as u8;
            }
        }
    }
    data.copy_from_slice(&tmp);
}
