//! Kept apart from the Wayland protocol so the buffer bookkeeping — refill in place, a second buffer only while the compositor holds the first, released at idle — can be driven in-process by tests.

use renderer_core::perf::{self, Phase};
use tiny_skia::PixmapMut;

use super::pixels::{PixelFormat, convert_rgba, convert_rgba_region, copy_region};
use super::present::{FrameOp, PresentLog};

pub(super) trait ShmBuffer {
    fn pixels(&self) -> &[u32];
    fn pixels_mut(&mut self) -> &mut [u32];
    // The compositor is done reading it, so it may be written.
    fn released(&self) -> bool;
}

// What the compositor reads from the buffers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum ShmLayout {
    // Premultiplied `[R, G, B, A]` bytes, tiny-skia's own: `wl_shm`'s `Abgr8888` on little-endian.
    Rgba,
    // Premultiplied `Argb8888`, which every compositor takes and which needs a conversion from tiny-skia's order.
    Argb,
}

// The protocol side of presenting: making and sizing buffers, hearing that the compositor released one, and committing one.
pub(super) trait Wire: Send {
    type Buffer: ShmBuffer + Send;

    fn create(&mut self, width: u32, height: u32) -> Option<Self::Buffer>;
    fn resize(&mut self, buffer: &mut Self::Buffer, width: u32, height: u32) -> bool;
    // Runs the events that have already arrived, so every `released` is current.
    fn dispatch_pending(&mut self);
    // Blocks until an event arrives; `false` once the connection is gone.
    fn wait(&mut self) -> bool;
    // Attaches `buffer`, marking it held until the compositor releases it, and damages `changed`.
    fn commit(&mut self, buffer: &mut Self::Buffer, changed: &FrameOp, width: u32, height: u32);
}

struct Slot<B> {
    buffer: B,
    // Presents since the buffer was last filled: 1 for the one on screen, 0 when its contents are unknown.
    age: u8,
    size: (usize, usize),
}

impl<B> Slot<B> {
    fn new(buffer: B) -> Self {
        Self {
            buffer,
            age: 0,
            size: (0, 0),
        }
    }

    fn holds_a_frame_of(&self, size: (usize, usize)) -> bool {
        self.age != 0 && self.size == size
    }
}

pub(super) struct Swapchain<B> {
    front: Slot<B>,
    back: Option<Slot<B>>,
    // This frame refills the front, which the compositor has released.
    in_place: bool,
    // Pixels copied by whole-buffer catch-ups, plus one per region copied.
    #[cfg(test)]
    pub(super) copied: usize,
}

impl<B: ShmBuffer> Swapchain<B> {
    pub(super) fn new(first: B) -> Self {
        Self {
            front: Slot::new(first),
            back: None,
            in_place: true,
            #[cfg(test)]
            copied: 0,
        }
    }

    fn needs_back(&self) -> bool {
        !self.front.buffer.released() && self.back.is_none()
    }

    fn ready(&self) -> bool {
        self.front.buffer.released()
            || self
                .back
                .as_ref()
                .is_some_and(|back| back.buffer.released())
    }

    // Only once `ready`: the front when the compositor has released it, which then needs no catching up, else the back.
    fn select(&mut self) -> Option<&mut B> {
        self.in_place = self.front.buffer.released();
        self.target_mut()
    }

    fn target_slot_mut(&mut self) -> Option<&mut Slot<B>> {
        if self.in_place {
            Some(&mut self.front)
        } else {
            self.back.as_mut()
        }
    }

    fn target_mut(&mut self) -> Option<&mut B> {
        self.target_slot_mut().map(|slot| &mut slot.buffer)
    }

    // Brings the selected buffer up to the frame on screen, copying from the front what it missed, unless `whole` says all of the next frame is drawn over it anyway. `false` when it was not caught up, so all of the next frame has to be drawn. Until `filled`, what the buffer holds counts as unknown, so a frame abandoned part way leaves nothing trusted behind.
    pub(super) fn catch_up(&mut self, log: &PresentLog, size: (usize, usize), whole: bool) -> bool {
        let caught_up = !whole && self.front.holds_a_frame_of(size) && self.copy_stale(log, size);
        if let Some(target) = self.target_slot_mut() {
            target.age = 0;
        }
        caught_up
    }

    fn copy_stale(&mut self, log: &PresentLog, size: (usize, usize)) -> bool {
        if self.in_place {
            return true;
        }
        let Some(back) = &mut self.back else {
            return false;
        };
        let back_age = if back.size == size { back.age } else { 0 };
        let stale = log.plan(back_age).stale;
        let (front, target) = (self.front.buffer.pixels(), back.buffer.pixels_mut());
        #[cfg(test)]
        {
            self.copied += match &stale {
                FrameOp::Full => front.len(),
                _ => stale.regions().len(),
            };
        }
        if matches!(stale, FrameOp::Full) {
            target.copy_from_slice(front);
        }
        for r in stale.regions() {
            copy_region(front, target, size.0, size.1, *r);
        }
        true
    }

    // Converts this frame's change into the selected buffer, or all of it when the buffer could not be caught up, and returns what the present declares.
    pub(super) fn convert(
        &mut self,
        changed: &FrameOp,
        caught_up: bool,
        rgba: &[u8],
        (width, height): (usize, usize),
    ) -> FrameOp {
        let Some(target) = self.target_mut() else {
            return FrameOp::NoChange;
        };
        let target = target.pixels_mut();
        if !caught_up || matches!(changed, FrameOp::Full) {
            convert_rgba(rgba, target, PixelFormat::Argb8888);
            return FrameOp::Full;
        }
        for r in changed.regions() {
            convert_rgba_region(rgba, target, width, height, *r, PixelFormat::Argb8888);
        }
        changed.clone()
    }

    // The selected buffer now holds the frame about to be committed, and becomes the front.
    pub(super) fn filled(&mut self, size: (usize, usize)) {
        if !self.in_place {
            let Some(back) = &mut self.back else {
                return;
            };
            std::mem::swap(&mut self.front, back);
        }
        self.front.age = 1;
        self.front.size = size;
        self.in_place = true;
        if let Some(back) = &mut self.back
            && back.age != 0
        {
            back.age = back.age.saturating_add(1);
        }
    }

    // Keeps the front, which is what the compositor shows and, drawing in place, the renderer's own last frame.
    fn release_back(&mut self) {
        self.back.take_if(|back| back.buffer.released());
    }

    #[cfg(test)]
    pub(super) fn buffers(&self) -> usize {
        1 + usize::from(self.back.is_some())
    }

    #[cfg(test)]
    pub(super) fn front(&self) -> &B {
        &self.front.buffer
    }
}

// A frame being drawn straight into a buffer, from `begin` to `present`.
#[derive(Clone, Copy)]
struct Drawing {
    width: u32,
    height: u32,
}

pub(super) struct ShmPresenter<W: Wire> {
    wire: W,
    layout: ShmLayout,
    chain: Option<Swapchain<W::Buffer>>,
    drawing: Option<Drawing>,
}

/// What the renderer asks of a transparent surface's presenter, whichever layout it negotiated.
pub(super) trait AlphaPresenter: Send {
    /// The renderer draws straight into the buffers, through [`begin`](Self::begin), [`target`](Self::target) and [`present`](Self::present), rather than handing over a pixmap to [`present_converted`](Self::present_converted).
    fn draws_in_place(&self) -> bool;

    /// Presents a premultiplied-RGBA frame (tiny-skia's pixmap byte order) as premultiplied `Argb8888`. `log` must already hold this frame's change, and is advanced only when the frame reaches the compositor.
    fn present_converted(&mut self, rgba: &[u8], width: u32, height: u32, log: &mut PresentLog);

    /// Picks the buffer this frame is drawn into and, unless `whole` says all of the frame is drawn anyway, brings it up to the frame on screen: `Some(false)` when that was not done and the whole of this frame must be drawn, `None` when there is no buffer to draw into.
    fn begin(&mut self, width: u32, height: u32, log: &PresentLog, whole: bool) -> Option<bool>;

    /// The buffer `begin` picked, as a pixmap to draw into.
    fn target(&mut self) -> Option<PixmapMut<'_>>;

    /// Commits what was drawn since `begin`, declaring `log`'s pending change, which must already hold this frame's. Does nothing without a `begin`.
    fn present(&mut self, log: &mut PresentLog);

    /// The last frame committed, as premultiplied RGBA bytes, when the buffers hold that layout.
    fn presented(&self) -> Option<&[u8]>;

    /// Forgets a frame `begin` started that will never be presented, leaving the buffer it was drawing into untrusted.
    fn abandon(&mut self);

    /// Drops the buffer only a frame in flight needs, keeping the one on screen. `false` while the compositor still holds it, so asking again later can still free it.
    fn release_idle(&mut self) -> bool;
}

impl<W: Wire> ShmPresenter<W> {
    pub(super) fn new(wire: W, layout: ShmLayout) -> Self {
        Self {
            wire,
            layout,
            chain: None,
            drawing: None,
        }
    }

    #[cfg(test)]
    pub(super) fn chain(&self) -> Option<&Swapchain<W::Buffer>> {
        self.chain.as_ref()
    }

    // The front when the compositor has released it, else a second buffer: made if there is none, waited for if the compositor holds both. Sized to the frame.
    fn acquire(&mut self, width: u32, height: u32) -> Option<&mut Swapchain<W::Buffer>> {
        if width == 0 || height == 0 {
            return None;
        }
        self.wire.dispatch_pending();
        if self.chain.is_none() {
            self.chain = Some(Swapchain::new(self.wire.create(width, height)?));
        }
        let chain = self.chain.as_mut()?;
        // Without a second buffer, the frame waits for the compositor to release the front.
        if chain.needs_back()
            && let Some(back) = self.wire.create(width, height)
        {
            chain.back = Some(Slot::new(back));
        }
        while !chain.ready() {
            if !self.wire.wait() {
                return None;
            }
        }
        let target = chain.select()?;
        self.wire.resize(target, width, height).then_some(chain)
    }

    fn commit(&mut self, changed: &FrameOp, width: u32, height: u32) {
        if let Some(chain) = &mut self.chain {
            self.wire
                .commit(&mut chain.front.buffer, changed, width, height);
        }
    }
}

fn rgba_bytes_mut(pixels: &mut [u32]) -> &mut [u8] {
    // SAFETY: `u8` has no alignment requirement and every byte of a `u32` is initialised, so the words read as their bytes in memory order.
    unsafe { pixels.align_to_mut::<u8>().1 }
}

fn rgba_bytes(pixels: &[u32]) -> &[u8] {
    // SAFETY: as in `rgba_bytes_mut`.
    unsafe { pixels.align_to::<u8>().1 }
}

impl<W: Wire> AlphaPresenter for ShmPresenter<W> {
    fn draws_in_place(&self) -> bool {
        self.layout == ShmLayout::Rgba
    }

    fn present_converted(&mut self, rgba: &[u8], width: u32, height: u32, log: &mut PresentLog) {
        let size = (width as usize, height as usize);
        let changed = {
            let _acquire = perf::span(Phase::Acquire);
            let Some(chain) = self.acquire(width, height) else {
                return;
            };
            let caught_up = chain.catch_up(log, size, matches!(log.pending(), FrameOp::Full));
            drop(_acquire);
            let _convert = perf::span(Phase::Convert);
            let changed = chain.convert(log.pending(), caught_up, rgba, size);
            chain.filled(size);
            changed
        };
        self.commit(&changed, width, height);
        log.presented();
    }

    fn begin(&mut self, width: u32, height: u32, log: &PresentLog, whole: bool) -> Option<bool> {
        let _acquire = perf::span(Phase::Acquire);
        self.drawing = None;
        let chain = self.acquire(width, height)?;
        let caught_up = chain.catch_up(log, (width as usize, height as usize), whole);
        self.drawing = Some(Drawing { width, height });
        Some(caught_up)
    }

    fn target(&mut self) -> Option<PixmapMut<'_>> {
        let Drawing { width, height } = self.drawing?;
        let pixels = self.chain.as_mut()?.target_mut()?.pixels_mut();
        PixmapMut::from_bytes(rgba_bytes_mut(pixels), width, height)
    }

    fn present(&mut self, log: &mut PresentLog) {
        let Some(Drawing { width, height }) = self.drawing.take() else {
            return;
        };
        let Some(chain) = &mut self.chain else {
            return;
        };
        chain.filled((width as usize, height as usize));
        self.commit(log.pending(), width, height);
        log.presented();
    }

    fn presented(&self) -> Option<&[u8]> {
        if self.layout != ShmLayout::Rgba || self.drawing.is_some() {
            return None;
        }
        let front = &self.chain.as_ref()?.front;
        (front.age == 1).then(|| rgba_bytes(front.buffer.pixels()))
    }

    fn abandon(&mut self) {
        self.drawing = None;
    }

    fn release_idle(&mut self) -> bool {
        self.wire.dispatch_pending();
        let Some(chain) = &mut self.chain else {
            return true;
        };
        chain.release_back();
        chain.back.is_none()
    }
}

#[cfg(test)]
#[path = "swapchain_test.rs"]
mod tests;
