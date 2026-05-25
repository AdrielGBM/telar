use std::sync::atomic::{AtomicU64, Ordering};

use wide::u32x4;

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
    /// RGBA8 pixels with premultiplied alpha. Premultiplication is applied automatically in `new()`.
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
        let mut pixels = pixels;
        premultiply_rgba(&mut pixels);
        Self {
            id: NEXT_IMAGE_ID.fetch_add(1, Ordering::Relaxed),
            pixels,
            width,
            height,
        }
    }
}

#[inline]
pub fn premultiply_rgba(pixels: &mut [u8]) {
    let mut iter = pixels.chunks_exact_mut(16);
    for chunk in iter.by_ref() {
        let r = u32x4::new([
            chunk[0] as u32,
            chunk[4] as u32,
            chunk[8] as u32,
            chunk[12] as u32,
        ]);
        let g = u32x4::new([
            chunk[1] as u32,
            chunk[5] as u32,
            chunk[9] as u32,
            chunk[13] as u32,
        ]);
        let b = u32x4::new([
            chunk[2] as u32,
            chunk[6] as u32,
            chunk[10] as u32,
            chunk[14] as u32,
        ]);
        let a = u32x4::new([
            chunk[3] as u32,
            chunk[7] as u32,
            chunk[11] as u32,
            chunk[15] as u32,
        ]);
        let bias = u32x4::splat(128);
        let shift = u32x4::splat(8);
        let r_new = ((r * a) + bias) >> shift;
        let g_new = ((g * a) + bias) >> shift;
        let b_new = ((b * a) + bias) >> shift;
        let ra = r_new.to_array();
        let ga = g_new.to_array();
        let ba = b_new.to_array();
        chunk[0] = ra[0] as u8;
        chunk[4] = ra[1] as u8;
        chunk[8] = ra[2] as u8;
        chunk[12] = ra[3] as u8;
        chunk[1] = ga[0] as u8;
        chunk[5] = ga[1] as u8;
        chunk[9] = ga[2] as u8;
        chunk[13] = ga[3] as u8;
        chunk[2] = ba[0] as u8;
        chunk[6] = ba[1] as u8;
        chunk[10] = ba[2] as u8;
        chunk[14] = ba[3] as u8;
    }
    for chunk in iter.into_remainder().chunks_exact_mut(4) {
        let a = chunk[3] as u32;
        chunk[0] = ((chunk[0] as u32 * a + 128) >> 8) as u8;
        chunk[1] = ((chunk[1] as u32 * a + 128) >> 8) as u8;
        chunk[2] = ((chunk[2] as u32 * a + 128) >> 8) as u8;
    }
}
