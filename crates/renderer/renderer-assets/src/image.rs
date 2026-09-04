//! Decoding a bitmap into the premultiplied pixels a renderer draws.

use renderer_core::Color;
#[cfg(feature = "dynamic-image")]
use renderer_core::ImageData;

#[cfg(feature = "dynamic-image")]
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

/// Decodes image bytes into premultiplied RGBA8 at runtime.
///
/// Which formats it understands is a build decision: PNG and JPEG always, plus whatever `image-*` feature the application turned on. A format nobody enabled comes back as an error here rather than failing to compile, so an app that opens files the user chose should say which formats it means.
#[cfg(feature = "dynamic-image")]
pub fn decode(bytes: &[u8]) -> Result<ImageData, ImageError> {
    let (rgba, w, h) = decode_rgba8(bytes)?;
    Ok(ImageData::new(rgba, w, h))
}

/// Build-time: decode PNG/JPEG bytes and emit a Rust expression constructing the equivalent `ImageData` (non-premultiplied RGBA8; `ImageData::new` premultiplies at load).
#[cfg(feature = "dynamic-image")]
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

#[cfg(feature = "dynamic-image")]
fn decode_rgba8(bytes: &[u8]) -> Result<(Vec<u8>, u32, u32), ImageError> {
    let img = image::load_from_memory(bytes).map_err(|e| ImageError(e.to_string()))?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    Ok((rgba.into_raw(), w, h))
}

/// A `b"..."` byte-string literal for the given bytes: far cheaper in compiler tokens than a `vec![1, 2, ...]` for a raster's worth of pixels. Every byte is emitted as `\xNN` so the output is always valid regardless of content.
#[cfg(any(feature = "dynamic-svg", feature = "dynamic-image"))]
pub(crate) fn byte_string_literal(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 4 + 3);
    out.push_str("b\"");
    for &b in bytes {
        out.push_str(&format!("\\x{b:02x}"));
    }
    out.push('"');
    out
}

/// The format knobs, checked where it matters: that turning one on actually reaches `image`.
///
/// A feature that forwards nowhere looks identical to one that works — `decode` keeps compiling either way, and the difference only shows as `unsupported format` on a user's file. GIF stands in for all thirteen: it is behind `image-gif` exactly as the others are behind theirs, and `image` can encode it, so the test can make its own input instead of carrying hand-written bytes for a format nobody can read in review.
#[cfg(all(test, feature = "image-gif"))]
mod format_features {
    #[test]
    fn a_format_behind_a_feature_decodes_once_that_feature_is_on() {
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
            .expect("the gif encoder is part of the same feature");

        let decoded = super::decode(&bytes).expect("gif decodes with `image-gif` on");
        assert_eq!((decoded.width, decoded.height), (2, 3));
    }
}
