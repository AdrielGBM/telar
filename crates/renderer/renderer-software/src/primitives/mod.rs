pub(crate) mod image;
pub(crate) mod line;
pub(crate) mod path;
pub(crate) mod rect;
pub(crate) mod text;

#[inline(always)]
pub(crate) fn to_skia_color(color: renderer_core::Color) -> tiny_skia::Color {
    tiny_skia::Color::from_rgba(
        color.r.clamp(0.0, 1.0),
        color.g.clamp(0.0, 1.0),
        color.b.clamp(0.0, 1.0),
        color.a.clamp(0.0, 1.0),
    )
    .expect("channels clamped to [0,1]")
}

pub(crate) fn to_skia_line_cap(cap: renderer_core::LineCap) -> tiny_skia::LineCap {
    match cap {
        renderer_core::LineCap::Butt => tiny_skia::LineCap::Butt,
        renderer_core::LineCap::Round => tiny_skia::LineCap::Round,
        renderer_core::LineCap::Square => tiny_skia::LineCap::Square,
    }
}

pub(crate) fn to_skia_line_join(join: renderer_core::LineJoin) -> tiny_skia::LineJoin {
    match join {
        renderer_core::LineJoin::Miter => tiny_skia::LineJoin::Miter,
        renderer_core::LineJoin::Round => tiny_skia::LineJoin::Round,
        renderer_core::LineJoin::Bevel => tiny_skia::LineJoin::Bevel,
    }
}

#[inline]
pub(crate) fn fill_to_paint(fill: renderer_core::FillStyle) -> tiny_skia::Paint<'static> {
    let mut paint = tiny_skia::Paint::default();
    paint.anti_alias = true;
    match fill {
        renderer_core::FillStyle::Solid(c) => {
            paint.set_color(to_skia_color(c));
        }
        renderer_core::FillStyle::LinearGradient(g) => {
            let mut stops = arrayvec::ArrayVec::<tiny_skia::GradientStop, 4>::new();
            for s in &g.stops[..g.stop_count as usize] {
                stops.push(tiny_skia::GradientStop::new(
                    s.position,
                    to_skia_color(s.color),
                ));
            }
            if let Some(shader) = tiny_skia::LinearGradient::new(
                tiny_skia::Point::from_xy(g.start.x, g.start.y),
                tiny_skia::Point::from_xy(g.end.x, g.end.y),
                stops.into_iter().collect::<Vec<_>>(),
                tiny_skia::SpreadMode::Pad,
                tiny_skia::Transform::identity(),
            ) {
                paint.shader = shader;
            }
        }
        renderer_core::FillStyle::RadialGradient(g) => {
            let mut stops = arrayvec::ArrayVec::<tiny_skia::GradientStop, 4>::new();
            for s in &g.stops[..g.stop_count as usize] {
                stops.push(tiny_skia::GradientStop::new(
                    s.position,
                    to_skia_color(s.color),
                ));
            }
            if let Some(shader) = tiny_skia::RadialGradient::new(
                tiny_skia::Point::from_xy(g.center.x, g.center.y),
                0.0,
                tiny_skia::Point::from_xy(g.center.x, g.center.y),
                g.radius,
                stops.into_iter().collect::<Vec<_>>(),
                tiny_skia::SpreadMode::Pad,
                tiny_skia::Transform::identity(),
            ) {
                paint.shader = shader;
            }
        }
    }
    paint
}

pub(crate) fn gaussian_blur(
    data: &mut [u8],
    width: u32,
    height: u32,
    sigma: f32,
    scratch: &mut Vec<u8>,
) {
    if sigma < 0.5 || width == 0 || height == 0 {
        return;
    }
    let r = ((sigma * 1.5).round() as u32).max(1);
    scratch.resize(data.len(), 0);
    for _ in 0..3 {
        box_blur_h(data, width, height, r, scratch);
        box_blur_v(data, width, height, r, scratch);
    }
}

fn box_blur_h(data: &mut [u8], width: u32, height: u32, r: u32, scratch: &mut Vec<u8>) {
    let w = width as usize;
    let h = height as usize;
    let r = r as usize;
    scratch.resize(data.len(), 0);
    if w == 0 || h == 0 {
        return;
    }
    for y in 0..h {
        let row = y * w;
        for c in 0..4usize {
            let initial_end = (r + 1).min(w);
            let mut sum: u32 = (0..initial_end)
                .map(|xi| data[(row + xi) * 4 + c] as u32)
                .sum();
            let mut count: u32 = initial_end as u32;
            for x in 0..w {
                scratch[(row + x) * 4 + c] = (sum / count) as u8;
                if x + r + 1 < w {
                    sum += data[(row + x + r + 1) * 4 + c] as u32;
                    count += 1;
                }
                if x >= r {
                    sum -= data[(row + (x - r)) * 4 + c] as u32;
                    count -= 1;
                }
            }
        }
    }
    data.copy_from_slice(&scratch[..data.len()]);
}

fn box_blur_v(data: &mut [u8], width: u32, height: u32, r: u32, scratch: &mut Vec<u8>) {
    let w = width as usize;
    let h = height as usize;
    let r = r as usize;
    scratch.resize(data.len(), 0);
    if w == 0 || h == 0 {
        return;
    }
    for x in 0..w {
        for c in 0..4usize {
            let initial_end = (r + 1).min(h);
            let mut sum: u32 = (0..initial_end)
                .map(|yi| data[(yi * w + x) * 4 + c] as u32)
                .sum();
            let mut count: u32 = initial_end as u32;
            for y in 0..h {
                scratch[(y * w + x) * 4 + c] = (sum / count) as u8;
                if y + r + 1 < h {
                    sum += data[((y + r + 1) * w + x) * 4 + c] as u32;
                    count += 1;
                }
                if y >= r {
                    sum -= data[((y - r) * w + x) * 4 + c] as u32;
                    count -= 1;
                }
            }
        }
    }
    data.copy_from_slice(&scratch[..data.len()]);
}
