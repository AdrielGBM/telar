#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ImageFilter {
    #[default]
    Nearest,
    Linear,
}

#[derive(Debug, Clone)]
pub struct ImageData {
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
            pixels,
            width,
            height,
        }
    }
}
