use wide::u32x4;
use xxhash_rust::xxh3::xxh3_64_with_seed;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ImageFilter {
    #[default]
    Nearest,
    Linear,
}

#[derive(Debug, Clone)]
pub struct ImageData {
    /// Content address: equal pixels at equal dimensions give equal ids, whoever built them and whenever.
    ///
    /// The renderers key their texture caches on this and nothing else, so the id has to identify the *image*. A per-construction counter identified the allocation instead: a caller that rebuilt the same image each frame — which is what building an `ImageData` inside a widget body does — minted a fresh key every time, and every entry behind it became unreachable weight the cache could only shed by hitting its byte budget.
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
        Self::addressed(pixels, width, height)
    }

    /// Builds from bytes that are ALREADY premultiplied (e.g. a resvg `Pixmap`), skipping the premultiply step `new()` performs.
    pub fn from_premultiplied(pixels: Vec<u8>, width: u32, height: u32) -> Self {
        assert_eq!(
            pixels.len(),
            (width * height * 4) as usize,
            "pixels must be RGBA8: width * height * 4 bytes"
        );
        Self::addressed(pixels, width, height)
    }

    // Hashed after premultiplication so both constructors address the same finished image alike. The dimensions ride in as the seed rather than as leading bytes: one buffer can be several images (a 4x1 and a 2x2 share their bytes), and seeding keeps that distinction while leaving the pixels a single one-shot pass.
    fn addressed(pixels: Vec<u8>, width: u32, height: u32) -> Self {
        let seed = ((width as u64) << 32) | height as u64;
        Self {
            id: xxh3_64_with_seed(&pixels, seed),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn opaque(pixels: &[[u8; 3]]) -> Vec<u8> {
        pixels
            .iter()
            .flat_map(|[r, g, b]| [*r, *g, *b, 255])
            .collect()
    }

    // The property the texture caches depend on: a widget body that rebuilds its image every frame must land on the entry it filled last frame, not mint a new one.
    #[test]
    fn the_same_image_built_twice_gets_the_same_id() {
        let once = ImageData::new(opaque(&[[10, 20, 30], [40, 50, 60]]), 2, 1);
        let again = ImageData::new(opaque(&[[10, 20, 30], [40, 50, 60]]), 2, 1);
        assert_eq!(once.id, again.id);
    }

    #[test]
    fn different_pixels_get_different_ids() {
        let a = ImageData::new(opaque(&[[10, 20, 30], [40, 50, 60]]), 2, 1);
        let b = ImageData::new(opaque(&[[10, 20, 30], [40, 50, 61]]), 2, 1);
        assert_ne!(a.id, b.id);
    }

    // Same bytes, different shape: the dimensions have to be part of the address or a 4x1 would be served the 2x2's texture.
    #[test]
    fn the_same_bytes_at_different_dimensions_get_different_ids() {
        let wide = ImageData::new(
            opaque(&[[1, 2, 3], [4, 5, 6], [7, 8, 9], [10, 11, 12]]),
            4,
            1,
        );
        let square = ImageData::new(
            opaque(&[[1, 2, 3], [4, 5, 6], [7, 8, 9], [10, 11, 12]]),
            2,
            2,
        );
        assert_ne!(wide.id, square.id);
    }

    // `new` premultiplies and `from_premultiplied` does not, so addressing has to happen after that step or the two would disagree about an image they both finished identically.
    #[test]
    fn both_constructors_address_the_same_finished_image_alike() {
        let half_alpha = vec![200, 100, 50, 128];
        let mut premultiplied = half_alpha.clone();
        premultiply_rgba(&mut premultiplied);
        assert_eq!(
            ImageData::new(half_alpha, 1, 1).id,
            ImageData::from_premultiplied(premultiplied, 1, 1).id
        );
    }
}
