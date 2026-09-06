//! Writing a captured frame buffer out as a PNG.

use std::path::Path;

use tiny_skia::{IntSize, Pixmap};

/// `pixels` must already be premultiplied RGBA8, as a compositor's frame sink hands out — `Pixmap::save_png` demultiplies before encoding, so premultiplying again here would double it.
pub fn save_premultiplied_rgba8_png(
    pixels: Vec<u8>,
    width: u32,
    height: u32,
    path: &Path,
) -> Result<(), String> {
    let size = IntSize::from_wh(width, height)
        .ok_or_else(|| "frame size does not match the window".to_string())?;
    Pixmap::from_vec(pixels, size)
        .ok_or_else(|| "frame size does not match the window".to_string())?
        .save_png(path)
        .map_err(|e| e.to_string())
}
