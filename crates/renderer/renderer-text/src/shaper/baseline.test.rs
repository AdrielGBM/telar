use super::*;
use renderer_core::{Color, FontAsset, FontFamily, Raster};

const TALL: &str = "Agjpy bdfhkl";

/// Faces that declare more ascent and descent than a line holds, so centring what they declare puts ink outside the line: one hangs its descenders out of the bottom, one lifts its ascenders out of the top.
const OVERFLOWING: [(&str, &[u8]); 2] = [
    (
        "Telar Test High Ascent",
        include_bytes!("../../test-fonts/TelarTestHighAscent.ttf"),
    ),
    (
        "Telar Test Deep Descent",
        include_bytes!("../../test-fonts/TelarTestDeepDescent.ttf"),
    ),
];

fn declared_extent(bytes: &[u8]) -> f32 {
    use skrifa::instance::{LocationRef, Size};
    let font = skrifa::FontRef::new(bytes).expect("the fixture parses");
    let metrics = skrifa::metrics::Metrics::new(&font, Size::new(1.0), LocationRef::default());
    metrics.ascent - metrics.descent
}

/// The last row a single line of `style` inks when drawn at the top of a box two lines tall, so nothing it draws below its own line is cut off.
fn last_drawn_row(shaper: &mut TextShaper, style: &TextStyle, line: f32) -> Option<usize> {
    let rect = Rect {
        x: 0.0,
        y: 0.0,
        width: 1_000.0,
        height: 2.0 * line,
    };
    let (pixels, width, _) = shaper.rasterize(TALL, None, rect, style);
    pixels
        .as_chunks::<4>()
        .0
        .chunks(width as usize)
        .rposition(|row| row.iter().any(|pixel| pixel[3] > 0))
}

/// A line keeps the ink of its tallest and deepest letters inside its own line box, whatever ascent and descent its face declares.
#[test]
fn a_line_keeps_its_ink_inside_its_line_box_whatever_its_face_declares() {
    {
        let _installing = crate::fonts::FACES_STABLE
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        crate::fonts::add_faces(
            OVERFLOWING
                .iter()
                .map(|(family, bytes)| FontAsset::embedded(bytes).named(*family))
                .collect(),
        );
    }
    let mut shaper = TextShaper::new();
    for (family, bytes) in OVERFLOWING {
        assert!(
            declared_extent(bytes) > crate::LINE_HEIGHT_FACTOR,
            "{family} must declare more than a line holds, or this test proves nothing"
        );
        for raster in [Raster::Smooth, Raster::Pixel] {
            for size in [11.0_f32, 13.0, 16.0, 24.0, 40.0] {
                let style = TextStyle::new(size, Color::BLACK)
                    .with_font_family(FontFamily::Named(family.into()))
                    .with_raster(raster);
                let (_, line) = shaper.measure_text(TALL, None, 1_000.0, &style);
                let (top, height) = shaper.measure_ink_bounds(TALL, 1_000.0, &style);
                assert!(
                    top >= 0.0 && top + height <= line,
                    "{family} at {size}px ({raster:?}) inks from {top} to {} in a {line}px line",
                    top + height
                );
                let last = last_drawn_row(&mut shaper, &style, line).expect("the line draws");
                assert!(
                    last as f32 + 1.0 <= line,
                    "{family} at {size}px ({raster:?}) draws row {last} below a {line}px line"
                );
            }
        }
    }
}
