use geometry_core::Rect;
#[cfg(target_os = "android")]
use raw_window_handle::HasWindowHandle;
#[cfg(target_os = "android")]
use renderer_core::RendererError;
use smallvec::SmallVec;
#[cfg(target_os = "android")]
use tiny_skia::Pixmap;

// What changed on screen in a presented frame relative to the previous one, recorded per frame so the damage-aware present path can refresh an aged softbuffer buffer (age N = the buffer we presented N frames ago) by re-swizzling only the union of the last N frames' changed regions. A scroll is recorded as Regions covering the whole scrolled clip (every pixel in it moved) plus the displaced overlays — re-swizzling from the already-shifted pixmap is cheaper than shifting the slow shared-memory present buffer in place.
#[derive(Clone)]
pub(super) enum FrameOp {
    // Nothing changed (skip frame): contributes no damage.
    NoChange,
    // The whole framebuffer was rewritten (first frame, resize, clear-color change, or a non-incremental redraw).
    Full,
    // Only these window-space regions changed.
    Regions(SmallVec<[Rect; 8]>),
}

// How to refresh a softbuffer buffer of the given age from the current pixmap.
pub(super) enum PresentPlan {
    // Re-swizzle the whole pixmap (safe fallback).
    Full,
    // Re-swizzle just these regions; the rest of the aged buffer is already current.
    Regions(SmallVec<[Rect; 8]>),
}

// Decides how to refresh a buffer of the given `age` from history, which must already include the current frame's op as its last entry. Any ambiguity (age 0 → undefined contents, too little history, or a Full anywhere in the window) falls back to a full re-swizzle, which is always correct.
pub(super) fn plan_present(history: &std::collections::VecDeque<FrameOp>, age: u8) -> PresentPlan {
    let k = age as usize;
    if k == 0 || k > history.len() {
        return PresentPlan::Full;
    }
    // The last k ops are exactly the frames missing from this aged buffer; the union of their changed regions is everything that differs from it.
    let mut regions: SmallVec<[Rect; 8]> = SmallVec::new();
    for op in history.iter().rev().take(k) {
        match op {
            FrameOp::Full => return PresentPlan::Full,
            FrameOp::NoChange => {}
            FrameOp::Regions(rs) => regions.extend(rs.iter().copied()),
        }
    }
    PresentPlan::Regions(regions)
}

// Clones the ANativeWindow out of a window handle so the renderer can present straight to it, bypassing softbuffer's intermediate buffer. Returns None off-Android or for any non-AndroidNdk handle.
#[cfg(target_os = "android")]
pub(super) fn extract_native_window<W: HasWindowHandle>(
    window: &W,
) -> Option<ndk::native_window::NativeWindow> {
    use raw_window_handle::RawWindowHandle;
    let handle = window.window_handle().ok()?;
    match handle.as_raw() {
        RawWindowHandle::AndroidNdk(a) => {
            // Safety: the handle is valid for the lifetime of the window; clone_from_ptr acquires its own reference.
            Some(unsafe {
                ndk::native_window::NativeWindow::clone_from_ptr(a.a_native_window.cast())
            })
        }
        _ => None,
    }
}

// Presents by copying the tiny_skia pixmap directly into the locked ANativeWindow buffer. tiny_skia is RGBA8888 and the window is configured RGBX8888 (same byte layout), so each visible row is a single memcpy — no per-pixel conversion. The lock guard posts the buffer on drop.
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
            // Safe: copy_from_slice only writes; every byte of `dst` is initialized from `src`.
            let dst: &mut [u8] =
                unsafe { &mut *(dst as *mut [std::mem::MaybeUninit<u8>] as *mut [u8]) };
            dst.copy_from_slice(&src[src_off..src_off + copy_bytes]);
        }
    }
    Ok(())
}
