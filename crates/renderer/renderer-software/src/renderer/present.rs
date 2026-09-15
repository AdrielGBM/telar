//! Presenting a frame, and refreshing an aged buffer from only the regions that have changed since.

use std::collections::VecDeque;

use geometry_core::Rect;
#[cfg(target_os = "android")]
use raw_window_handle::HasWindowHandle;
#[cfg(target_os = "android")]
use renderer_core::RendererError;
use smallvec::{SmallVec, smallvec};
#[cfg(target_os = "android")]
use tiny_skia::Pixmap;

use super::pixels::{PixelFormat, clamp_to_pixels, convert_rgba, convert_rgba_region};

// Deeper than any buffer age a surface reports in practice; an older buffer is refreshed in full.
const HISTORY: usize = 6;

// A scroll is recorded as regions covering the whole scrolled clip plus the displaced overlays: re-converting from the already-shifted pixmap is cheaper than shifting the shared-memory present buffer in place.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum FrameOp {
    NoChange,
    // First frame, resize, clear-colour change, or a non-incremental redraw.
    Full,
    // Only these window-space regions changed.
    Regions(SmallVec<[Rect; 8]>),
}

impl FrameOp {
    pub(super) fn merge(self, other: FrameOp) -> FrameOp {
        match (self, other) {
            (FrameOp::Full, _) | (_, FrameOp::Full) => FrameOp::Full,
            (FrameOp::NoChange, op) | (op, FrameOp::NoChange) => op,
            (FrameOp::Regions(mut regions), FrameOp::Regions(more)) => {
                regions.extend(more);
                // Collapsed past the inline capacity, so frames that never reach a present cannot grow the list without bound.
                if regions.spilled()
                    && let Some(bounds) = regions.iter().copied().reduce(Rect::union)
                {
                    regions = smallvec![bounds];
                }
                FrameOp::Regions(regions)
            }
        }
    }

    pub(super) fn regions(&self) -> &[Rect] {
        match self {
            FrameOp::Regions(regions) => regions,
            FrameOp::NoChange | FrameOp::Full => &[],
        }
    }

    // The whole-pixel rects this covers on a `width` x `height` surface, `None` meaning all of it.
    pub(super) fn pixel_rects(&self, width: u32, height: u32) -> Option<SmallVec<[PixelRect; 8]>> {
        match self {
            FrameOp::Full => None,
            FrameOp::NoChange => Some(SmallVec::new()),
            FrameOp::Regions(regions) => Some(
                regions
                    .iter()
                    .filter_map(|r| clamp_to_pixels(*r, width, height))
                    .map(|(x0, y0, x1, y1)| PixelRect {
                        x: x0,
                        y: y0,
                        width: x1 - x0,
                        height: y1 - y0,
                    })
                    .collect(),
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PixelRect {
    pub(super) x: u32,
    pub(super) y: u32,
    pub(super) width: u32,
    pub(super) height: u32,
}

#[derive(Clone, Copy)]
pub(super) enum SurfaceDamage {
    // The surface may lose what was presented, so every present declares all of it.
    Whole,
    // Contents are kept but damage has no per-rect form: `wl_surface` below version 4.
    #[cfg(target_os = "linux")]
    AllOrNothing,
    Rects,
}

// `None` declares the whole surface.
pub(super) fn declared_damage(
    changed: &FrameOp,
    surface: SurfaceDamage,
    width: u32,
    height: u32,
) -> Option<SmallVec<[PixelRect; 8]>> {
    match surface {
        SurfaceDamage::Whole => None,
        #[cfg(target_os = "linux")]
        SurfaceDamage::AllOrNothing => changed
            .pixel_rects(width, height)
            .filter(|rects| rects.is_empty()),
        SurfaceDamage::Rects => changed.pixel_rects(width, height),
    }
}

pub(super) fn note_damage(rects: Option<&[PixelRect]>, width: u32, height: u32) {
    let surface = u64::from(width) * u64::from(height);
    let damaged = rects.map_or(surface, |rects| {
        rects
            .iter()
            .map(|r| u64::from(r.width) * u64::from(r.height))
            .sum::<u64>()
            .min(surface)
    });
    renderer_core::perf::note_damage(rects.is_some());
    renderer_core::perf::note_damage_area(damaged, surface);
}

// Only frames that reach a present are logged, so a buffer's age indexes it exactly.
pub(super) struct PresentLog {
    presented: VecDeque<FrameOp>,
    pending: FrameOp,
}

pub(super) struct PresentPlan {
    pub(super) stale: FrameOp,
    pub(super) changed: FrameOp,
}

impl PresentLog {
    pub(super) fn new() -> Self {
        Self {
            presented: VecDeque::with_capacity(HISTORY),
            pending: FrameOp::Full,
        }
    }

    pub(super) fn record(&mut self, op: FrameOp) {
        self.pending = std::mem::replace(&mut self.pending, FrameOp::NoChange).merge(op);
    }

    // `age` counts the presents since a buffer was last filled, 0 meaning its contents are unknown. Too little history falls back to a full refresh, which is always correct.
    pub(super) fn plan(&self, age: u8) -> PresentPlan {
        let stale = match usize::from(age).checked_sub(1) {
            Some(missed) if missed <= self.presented.len() => self
                .presented
                .iter()
                .rev()
                .take(missed)
                .cloned()
                .fold(FrameOp::NoChange, FrameOp::merge),
            _ => FrameOp::Full,
        };
        PresentPlan {
            stale,
            changed: self.pending.clone(),
        }
    }

    pub(super) fn presented(&mut self) {
        let op = std::mem::replace(&mut self.pending, FrameOp::NoChange);
        self.presented.push_back(op);
        if self.presented.len() > HISTORY {
            self.presented.pop_front();
        }
    }

    pub(super) fn reset(&mut self) {
        self.presented.clear();
        self.pending = FrameOp::Full;
    }
}

impl PresentPlan {
    pub(super) fn refresh(
        &self,
        rgba: &[u8],
        buffer: &mut [u32],
        width: usize,
        height: usize,
        format: PixelFormat,
    ) {
        if matches!(self.stale, FrameOp::Full) || matches!(self.changed, FrameOp::Full) {
            convert_rgba(rgba, buffer, format);
            return;
        }
        for r in self.stale.regions().iter().chain(self.changed.regions()) {
            convert_rgba_region(rgba, buffer, width, height, *r, format);
        }
    }
}

// Bypasses softbuffer's intermediate buffer. `None` off Android or for any non-AndroidNdk handle.
#[cfg(target_os = "android")]
pub(super) fn extract_native_window<W: HasWindowHandle>(
    window: &W,
) -> Option<ndk::native_window::NativeWindow> {
    use raw_window_handle::RawWindowHandle;
    let handle = window.window_handle().ok()?;
    match handle.as_raw() {
        RawWindowHandle::AndroidNdk(a) => {
            // Safety: the handle is valid for the window's lifetime, and `clone_from_ptr` acquires its own reference.
            Some(unsafe {
                ndk::native_window::NativeWindow::clone_from_ptr(a.a_native_window.cast())
            })
        }
        _ => None,
    }
}

// tiny_skia is RGBA8888 and the window is configured RGBX8888, the same byte layout, so each visible row is a single memcpy. The lock guard posts the buffer on drop.
#[cfg(target_os = "android")]
pub(super) fn present_to_native_window(
    nw: &ndk::native_window::NativeWindow,
    pixmap: &Pixmap,
) -> Result<(), RendererError> {
    use ndk::hardware_buffer_format::HardwareBufferFormat;
    let mut guard = nw
        .lock(None)
        .map_err(|e| RendererError::Present(format!("ANativeWindow lock failed: {e}")))?;
    let fmt = guard.format();
    if !matches!(
        fmt,
        HardwareBufferFormat::R8G8B8A8_UNORM | HardwareBufferFormat::R8G8B8X8_UNORM
    ) {
        return Err(RendererError::Present(format!(
            "unexpected ANativeWindow format {fmt:?}"
        )));
    }
    let gw = guard.width();
    let src = pixmap.data();
    let src_w = pixmap.width() as usize;
    let src_h = pixmap.height() as usize;
    let copy_bytes = gw.min(src_w) * 4;
    if let Some(lines) = guard.lines() {
        for (y, out) in lines.enumerate() {
            if y >= src_h {
                break;
            }
            let src_off = y * src_w * 4;
            let dst = &mut out[..copy_bytes];
            // Safe: `copy_from_slice` only writes, and every byte of `dst` is initialised from `src`.
            let dst: &mut [u8] =
                unsafe { &mut *(dst as *mut [std::mem::MaybeUninit<u8>] as *mut [u8]) };
            dst.copy_from_slice(&src[src_off..src_off + copy_bytes]);
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "present_test.rs"]
mod tests;
