use geometry_core::Rect;

use super::pixels::{PixelFormat, convert_rgba};
use super::present::FrameOp;

// Seeds that differ give frames that differ in every pixel.
pub(super) fn pattern(width: usize, height: usize, seed: u32) -> Vec<u8> {
    (0..width * height)
        .flat_map(|i| {
            (i as u32)
                .wrapping_mul(2_654_435_761)
                .wrapping_add(seed)
                .to_le_bytes()
        })
        .collect()
}

pub(super) fn paint(rgba: &mut [u8], width: usize, op: &FrameOp, seed: u32) {
    let height = rgba.len() / 4 / width;
    let rects = match op {
        FrameOp::NoChange => Vec::new(),
        FrameOp::Full => vec![Rect::new(0.0, 0.0, width as f32, height as f32)],
        FrameOp::Regions(regions) => regions.to_vec(),
    };
    for rect in rects {
        for y in rect.y as usize..(rect.y + rect.height) as usize {
            for x in rect.x as usize..(rect.x + rect.width) as usize {
                let i = (y * width + x) * 4;
                let value = seed.wrapping_mul(0x0101_0101) ^ (x * 31 + y) as u32;
                rgba[i..i + 4].copy_from_slice(&value.to_le_bytes());
            }
        }
    }
}

pub(super) fn converted(rgba: &[u8], format: PixelFormat) -> Vec<u32> {
    let mut out = vec![0; rgba.len() / 4];
    convert_rgba(rgba, &mut out, format);
    out
}
