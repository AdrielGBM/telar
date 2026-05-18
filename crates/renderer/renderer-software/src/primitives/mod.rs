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
