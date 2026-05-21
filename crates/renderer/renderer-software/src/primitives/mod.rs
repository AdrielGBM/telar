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
