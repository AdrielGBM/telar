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

#[cfg(target_os = "linux")]
pub(super) use memory::{Compositor, MemoryWire, Screen};

// An in-process stand-in for the compositor, so the presenter can be driven without one.
#[cfg(target_os = "linux")]
mod memory {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex, PoisonError};

    use super::super::present::FrameOp;
    use super::super::swapchain::{ShmBuffer, Wire};

    // What a fresh or resized buffer holds, a value no frame draws.
    const POISON: u32 = 0xDEAD_BEEF;

    #[derive(Clone, Copy, Debug, PartialEq)]
    pub(in super::super) enum Compositor {
        // Copies what a commit damages and releases the buffer at once, as wlroots does with shm.
        Copies,
        // Reads a committed buffer until the next commit replaces it.
        Holds,
        // Holds a committed buffer as `Holds` does, but releases the one a commit replaced only when the test says the release arrived.
        Lags,
    }

    // What the compositor shows, updated only where a commit declares damage, plus what it saw happen.
    #[derive(Default)]
    pub(in super::super) struct Screen {
        pub(in super::super) pixels: Vec<u32>,
        pub(in super::super) size: (usize, usize),
        pub(in super::super) commits: Vec<FrameOp>,
        pub(in super::super) created: usize,
        pub(in super::super) alive: usize,
        // Makes every buffer the presenter asks for fail to be created.
        pub(in super::super) refuse_buffers: bool,
        lagging: Vec<Arc<AtomicBool>>,
    }

    impl Screen {
        pub(in super::super) fn release_lagging(&mut self) {
            for replaced in self.lagging.drain(..) {
                replaced.store(true, Ordering::SeqCst);
            }
        }

        pub(in super::super) fn bytes(&self) -> Vec<u8> {
            self.pixels.iter().flat_map(|px| px.to_le_bytes()).collect()
        }
    }

    pub(in super::super) struct MemoryBuffer {
        pixels: Vec<u32>,
        size: (usize, usize),
        released: Arc<AtomicBool>,
        screen: Arc<Mutex<Screen>>,
    }

    impl ShmBuffer for MemoryBuffer {
        fn pixels(&self) -> &[u32] {
            &self.pixels
        }

        fn pixels_mut(&mut self) -> &mut [u32] {
            &mut self.pixels
        }

        fn released(&self) -> bool {
            self.released.load(Ordering::SeqCst)
        }
    }

    impl Drop for MemoryBuffer {
        fn drop(&mut self) {
            self.screen
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .alive -= 1;
        }
    }

    pub(in super::super) struct MemoryWire {
        compositor: Compositor,
        screen: Arc<Mutex<Screen>>,
        held: Option<Arc<AtomicBool>>,
    }

    impl MemoryWire {
        pub(in super::super) fn new(compositor: Compositor) -> (Self, Arc<Mutex<Screen>>) {
            let screen = Arc::new(Mutex::new(Screen::default()));
            let wire = Self {
                compositor,
                screen: Arc::clone(&screen),
                held: None,
            };
            (wire, screen)
        }
    }

    impl Wire for MemoryWire {
        type Buffer = MemoryBuffer;

        fn create(&mut self, width: u32, height: u32) -> Option<MemoryBuffer> {
            let mut screen = self.screen.lock().unwrap();
            if screen.refuse_buffers {
                return None;
            }
            screen.created += 1;
            screen.alive += 1;
            let size = (width as usize, height as usize);
            Some(MemoryBuffer {
                pixels: vec![POISON; size.0 * size.1],
                size,
                released: Arc::new(AtomicBool::new(true)),
                screen: Arc::clone(&self.screen),
            })
        }

        fn resize(&mut self, buffer: &mut MemoryBuffer, width: u32, height: u32) -> bool {
            let size = (width as usize, height as usize);
            if buffer.size != size {
                buffer.pixels = vec![POISON; size.0 * size.1];
                buffer.size = size;
            }
            true
        }

        fn dispatch_pending(&mut self) {}

        fn wait(&mut self) -> bool {
            let mut screen = self.screen.lock().unwrap();
            if !screen.lagging.is_empty() {
                screen.release_lagging();
                return true;
            }
            self.held
                .take()
                .map(|held| held.store(true, Ordering::SeqCst))
                .is_some()
        }

        fn commit(
            &mut self,
            buffer: &mut MemoryBuffer,
            changed: &FrameOp,
            width: u32,
            height: u32,
        ) {
            let mut screen = self.screen.lock().unwrap();
            let size = (width as usize, height as usize);
            let rects = if screen.size == size {
                changed.pixel_rects(width, height)
            } else {
                None
            };
            match rects {
                None => screen.pixels = buffer.pixels.clone(),
                Some(rects) => {
                    for r in rects {
                        for y in r.y as usize..(r.y + r.height) as usize {
                            let row = y * size.0;
                            let span = row + r.x as usize..row + (r.x + r.width) as usize;
                            screen.pixels[span.clone()].copy_from_slice(&buffer.pixels[span]);
                        }
                    }
                }
            }
            screen.size = size;
            screen.commits.push(changed.clone());
            if self.compositor == Compositor::Copies {
                return;
            }
            buffer.released.store(false, Ordering::SeqCst);
            if let Some(replaced) = self.held.replace(Arc::clone(&buffer.released))
                && !Arc::ptr_eq(&replaced, &buffer.released)
            {
                match self.compositor {
                    Compositor::Lags => screen.lagging.push(replaced),
                    _ => replaced.store(true, Ordering::SeqCst),
                }
            }
        }
    }
}
