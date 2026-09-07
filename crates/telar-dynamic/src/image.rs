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
#[path = "image_test.rs"]
mod tests;
