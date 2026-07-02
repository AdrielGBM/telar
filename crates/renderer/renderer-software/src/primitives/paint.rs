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
pub(crate) fn fill_to_paint(fill: renderer_core::Paint) -> tiny_skia::Paint<'static> {
    let mut paint = tiny_skia::Paint::default();
    paint.anti_alias = true;
    match fill {
        renderer_core::Paint::Solid(c) => {
            paint.set_color(to_skia_color(c));
        }
        renderer_core::Paint::Gradient(g) => {
            let mut skia_stops = Vec::with_capacity(8);
            skia_stops.extend(
                g.stops
                    .active()
                    .iter()
                    .map(|s| tiny_skia::GradientStop::new(s.position, to_skia_color(s.color))),
            );
            match g.kind {
                renderer_core::GradientKind::Linear { start, end } => {
                    if let Some(shader) = tiny_skia::LinearGradient::new(
                        tiny_skia::Point::from_xy(start.x, start.y),
                        tiny_skia::Point::from_xy(end.x, end.y),
                        skia_stops,
                        tiny_skia::SpreadMode::Pad,
                        tiny_skia::Transform::identity(),
                    ) {
                        paint.shader = shader;
                    }
                }
                renderer_core::GradientKind::Radial { center, radius } => {
                    if let Some(shader) = tiny_skia::RadialGradient::new(
                        tiny_skia::Point::from_xy(center.x, center.y),
                        0.0,
                        tiny_skia::Point::from_xy(center.x, center.y),
                        radius,
                        skia_stops,
                        tiny_skia::SpreadMode::Pad,
                        tiny_skia::Transform::identity(),
                    ) {
                        paint.shader = shader;
                    }
                }
            }
        }
    }
    paint
}
