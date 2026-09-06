//! Baking a bitmap into the premultiplied pixels a renderer draws.

use renderer_core::Color;

#[cfg(feature = "bake")]
#[derive(Debug, Clone, thiserror::Error)]
#[error("failed to decode image: {0}")]
/// A bitmap could not be decoded.
pub struct ImageError(String);

/// Multiplies a premultiplied-RGBA8 buffer by `tint` (srcIn): the buffer's alpha is the source coverage, and the tint's own alpha scales it.
pub(crate) fn apply_tint_premultiplied(pixels: &mut [u8], tint: Color) {
    for px in pixels.chunks_exact_mut(4) {
        // Buffer is premultiplied, so its alpha byte already equals the source coverage.
        let coverage = px[3] as f32 / 255.0;
        let out_a = coverage * tint.a;
        px[0] = (tint.r * out_a * 255.0).round().clamp(0.0, 255.0) as u8;
        px[1] = (tint.g * out_a * 255.0).round().clamp(0.0, 255.0) as u8;
        px[2] = (tint.b * out_a * 255.0).round().clamp(0.0, 255.0) as u8;
        px[3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
    }
}

/// Build-time: decode PNG/JPEG bytes and emit a Rust expression constructing the equivalent `ImageData` (non-premultiplied RGBA8; `ImageData::new` premultiplies at load).
#[cfg(feature = "bake")]
pub fn bake_image_to_source(bytes: &[u8]) -> Result<String, ImageError> {
    let (rgba, w, h) = decode_rgba8(bytes)?;
    // Bare `ImageData` (not `::renderer_core::ImageData`): the transpiler drops this into code that does `use telar::*`, whose facade re-exports the type unqualified.
    Ok(format!(
        "ImageData::new({}.to_vec(), {}, {})",
        byte_string_literal(&rgba),
        w,
        h
    ))
}

#[cfg(feature = "bake")]
fn decode_rgba8(bytes: &[u8]) -> Result<(Vec<u8>, u32, u32), ImageError> {
    let img = image::load_from_memory(bytes).map_err(|e| ImageError(e.to_string()))?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    Ok((rgba.into_raw(), w, h))
}

/// A `b"..."` byte-string literal for the given bytes: far cheaper in compiler tokens than a `vec![1, 2, ...]` for a raster's worth of pixels. Every byte is emitted as `\xNN` so the output is always valid regardless of content.
#[cfg(feature = "bake")]
pub(crate) fn byte_string_literal(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 4 + 3);
    out.push_str("b\"");
    for &b in bytes {
        out.push_str(&format!("\\x{b:02x}"));
    }
    out.push('"');
    out
}

/// The baker takes every format `image` ships, and it has to: which format an `img src:"…"` points at is not knowable from the feature list, and a build-time failure over a missing decoder would be a compile error on a file that is perfectly valid. GIF stands in for the set — nothing enables it individually here, so it only decodes because `bake` asks for all of them.
#[cfg(all(test, feature = "bake"))]
mod baked_formats {
    #[test]
    fn the_baker_reads_a_format_no_runtime_knob_would_have_turned_on() {
        let source = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            2,
            3,
            image::Rgba([10, 20, 30, 255]),
        ));
        let mut bytes = Vec::new();
        source
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Gif,
            )
            .expect("the gif encoder comes with `bake`");

        let (rgba, w, h) = super::decode_rgba8(&bytes).expect("gif decodes for the baker");
        assert_eq!((w, h), (2, 3));
        assert_eq!(rgba.len(), 2 * 3 * 4);
    }
}
