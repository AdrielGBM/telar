//! Decoding a bitmap into the premultiplied pixels a renderer draws.

use std::io::Cursor;
use std::sync::Arc;

use image::{ImageReader, Limits};
use renderer_core::ImageData;
use ui_core::{AssetDecoder, AssetError};

const MAX_DIMENSION: u32 = 8192;
const MAX_DECODE_BYTES: u64 = 256 * 1024 * 1024;

/// Decodes bitmap bytes into the `Arc<ImageData>` the `Image` widget takes.
///
/// A side beyond 8192 pixels, or a decode needing more than 256 MiB, is refused before the pixels are allocated, so a small hostile file cannot claim a huge raster.
///
/// Which formats it understands is a build decision: PNG and JPEG always, plus whatever `image-*` feature the application turned on. A format nobody enabled is not a compile error — `image` dispatches on what was compiled in, so it comes back as a decode failure here, on the one file the user happened to open.
pub struct ImageDecoder;

impl AssetDecoder for ImageDecoder {
    fn kind(&self) -> &'static str {
        "image"
    }

    type Output = Arc<ImageData>;

    fn decode(&self, bytes: &[u8]) -> Result<Self::Output, AssetError> {
        let fail = |e: &dyn std::fmt::Display| {
            AssetError::permanent(format!("failed to decode image: {e}"))
        };
        let mut reader = ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .map_err(|e| fail(&e))?;
        let mut limits = Limits::default();
        limits.max_image_width = Some(MAX_DIMENSION);
        limits.max_image_height = Some(MAX_DIMENSION);
        limits.max_alloc = Some(MAX_DECODE_BYTES);
        reader.limits(limits);
        let rgba = reader.decode().map_err(|e| fail(&e))?.into_rgba8();
        let (width, height) = rgba.dimensions();
        Ok(Arc::new(ImageData::new(rgba.into_raw(), width, height)))
    }
}

#[cfg(test)]
#[path = "image_test.rs"]
mod tests;
