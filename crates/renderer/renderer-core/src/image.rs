use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_IMAGE_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ImageFilter {
    #[default]
    Nearest,
    Linear,
}

#[derive(Debug, Clone)]
pub struct ImageData {
    pub id: u64,
    /// RGBA8 pixels with straight (non-premultiplied) alpha.
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl ImageData {
    pub fn new(pixels: Vec<u8>, width: u32, height: u32) -> Self {
        assert_eq!(
            pixels.len(),
            (width * height * 4) as usize,
            "pixels must be RGBA8: width * height * 4 bytes"
        );
        Self {
            id: NEXT_IMAGE_ID.fetch_add(1, Ordering::Relaxed),
            pixels,
            width,
            height,
        }
    }
}

pub fn premultiply_rgba(pixels: &mut [u8]) {
    for chunk in pixels.chunks_exact_mut(4) {
        let a = chunk[3] as u32;
        chunk[0] = ((chunk[0] as u32 * a) / 255) as u8;
        chunk[1] = ((chunk[1] as u32 * a) / 255) as u8;
        chunk[2] = ((chunk[2] as u32 * a) / 255) as u8;
    }
}
