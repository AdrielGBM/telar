//! [`LentFrames`]: the frames a guest's instance paints, as its entries lend them.

use platform_core::guest::LentBytes;
use renderer_core::DrawCommand;
use renderer_core::wire::FrameEncoder;

/// The frames one instance lends its host, each encoded on top of the ones before it, with the pixels of the images the last one draws. The segment cache cannot be lent itself: its `Arc`s would be freed by whichever side dropped them last.
#[derive(Default)]
pub struct LentFrames(FrameEncoder);

impl LentFrames {
    pub fn new() -> Self {
        Self::default()
    }

    /// `commands` as the next frame, lent until the host next calls into this library, but for the pixels entry, which leaves it lent while the host reads the images it draws.
    pub fn paint(&mut self, commands: &[DrawCommand]) -> LentBytes {
        LentBytes::lend(self.0.encode(commands))
    }

    /// Writes the pixels of an image the last frame drew to `pixels`, and to `found` whether it drew one by that id: what the pixels entry answers.
    ///
    /// # Safety
    ///
    /// `pixels` and `found` must be writable.
    pub unsafe fn lend_pixels(&self, image: u64, pixels: *mut LentBytes, found: *mut bool) {
        let lent = self.0.pixels(image).map(LentBytes::lend);
        unsafe {
            found.write(lent.is_some());
            pixels.write(lent.unwrap_or_default());
        }
    }

    /// Has the next frame stand alone, for a host that could not read one of the frames before it.
    pub fn reset(&mut self) {
        self.0.reset();
    }
}
