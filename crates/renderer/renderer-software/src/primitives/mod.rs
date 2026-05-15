pub(crate) mod image;
pub(crate) mod line;
pub(crate) mod path;
pub(crate) mod rect;
pub(crate) mod text;

pub(crate) fn premultiply_rgba_in_place(pixels: &mut [u8]) {
    for chunk in pixels.chunks_exact_mut(4) {
        let a = chunk[3] as u32;
        chunk[0] = ((chunk[0] as u32 * a) / 255) as u8;
        chunk[1] = ((chunk[1] as u32 * a) / 255) as u8;
        chunk[2] = ((chunk[2] as u32 * a) / 255) as u8;
    }
}
