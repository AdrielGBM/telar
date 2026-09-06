//! Decoding a bitmap into the premultiplied pixels a renderer draws.

use std::sync::Arc;

use renderer_core::ImageData;
use ui_core::{AssetDecoder, AssetError};

/// Decodes bitmap bytes into the `Arc<ImageData>` the `Image` widget takes.
///
/// Which formats it understands is a build decision: PNG and JPEG always, plus whatever `image-*` feature the application turned on. A format nobody enabled is not a compile error — `image` dispatches on what was compiled in, so it comes back as a decode failure here, on the one file the user happened to open.
pub struct ImageDecoder;

impl AssetDecoder for ImageDecoder {
    fn kind(&self) -> &'static str {
        "image"
    }

    type Output = Arc<ImageData>;

    fn decode(&self, bytes: &[u8]) -> Result<Self::Output, AssetError> {
        let decoded = image::load_from_memory(bytes)
            .map_err(|e| AssetError(format!("failed to decode image: {e}")))?;
        let rgba = decoded.to_rgba8();
        let (width, height) = rgba.dimensions();
        Ok(Arc::new(ImageData::new(rgba.into_raw(), width, height)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_body_that_is_not_a_bitmap_fails_rather_than_producing_an_empty_raster() {
        assert!(ImageDecoder.decode(b"<html>404 Not Found</html>").is_err());
    }

    /// The format knobs, checked where it matters: that turning one on actually reaches `image`.
    ///
    /// A feature that forwards nowhere looks identical to one that works — [`ImageDecoder`] keeps compiling either way, and the difference only shows as a decode failure on a user's file. GIF stands in for all thirteen: it is behind `image-gif` exactly as the others are behind theirs, and `image` can encode it, so the test can make its own input instead of carrying hand-written bytes for a format nobody can read in review.
    #[cfg(feature = "image-gif")]
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

        let decoded = ImageDecoder
            .decode(&bytes)
            .expect("gif decodes with `image-gif` on");
        assert_eq!((decoded.width, decoded.height), (2, 3));
    }
}
