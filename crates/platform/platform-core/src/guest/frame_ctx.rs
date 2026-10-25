//! [`FrameCtx`]: what a host lends a guest's frame in place of an [`AppCtx`], whose borrows, `Arc` waker and `repr(Rust)` raw handles cannot cross.

use raw_window_handle::{RawDisplayHandle, RawWindowHandle};

use super::lent::LentBytes;
use super::waker::CWaker;
use crate::AppCtx;

/// One frame's context, lent for the call: a redraw the guest asks for, written back; the host's loop waker, borrowed, which the guest clones for the references it keeps; and the window's raw handles, spelled as bytes.
///
/// The handles cross as bytes rather than as `repr(C)` fields because `raw-window-handle`'s types are non-exhaustive `repr(Rust)` enums whose every variant has a shape of its own: spelled with named tags they cross like every other rich value, the context keeps one fixed layout, and a handle the spelling leaves out arrives as none, the answer [`AppCtx`] already gives for a surface that has no handle.
#[repr(C)]
pub struct FrameCtx {
    pub(super) redraw_requested: bool,
    pub(super) waker: *const CWaker,
    pub(super) handles: LentBytes,
}

/// A window's raw handles, as a [`FrameCtx`] spells them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct RawHandles {
    pub(crate) window: Option<RawWindowHandle>,
    pub(crate) display: Option<RawDisplayHandle>,
}

impl FrameCtx {
    /// The host's side: runs `call` with a context lent over `ctx`, and carries a redraw the guest asked for back to `ctx`. The waker it lends wakes through [`shield`](super::shield), so a panic in the host's loop stays on this side.
    pub fn lend<T>(ctx: &mut AppCtx<'_>, call: impl FnOnce(*mut FrameCtx) -> T) -> T {
        let waker = ctx.redraw_waker().map(CWaker::new);
        let handles = wire_core::encode(&RawHandles {
            window: ctx.raw_window_handle(),
            display: ctx.raw_display_handle(),
        });
        let mut frame = FrameCtx {
            redraw_requested: false,
            waker: waker.as_ref().map_or(std::ptr::null(), std::ptr::from_ref),
            handles: LentBytes::lend(&handles),
        };
        let out = call(&mut frame);
        if frame.redraw_requested {
            ctx.request_redraw();
        }
        out
    }

    /// The guest's side: runs `call` with an [`AppCtx`] of this side's own over the context the host lent, and writes a redraw it asked for back into it.
    ///
    /// # Safety
    ///
    /// `frame` must be the context the running entry was handed, used only inside that call.
    pub unsafe fn enter<T>(frame: *mut FrameCtx, call: impl FnOnce(&mut AppCtx<'_>) -> T) -> T {
        let frame = unsafe { &mut *frame };
        let waker = unsafe { frame.waker.as_ref() }.map(|waker| waker.clone().into_redraw_waker());
        let handles = unsafe { frame.handles.decode::<RawHandles>() }.unwrap_or_else(|error| {
            tracing::warn!(%error, "the host lent window handles this guest cannot read");
            RawHandles::default()
        });
        let mut redraw_requested = false;
        let mut ctx = AppCtx::new(
            &mut redraw_requested,
            waker.as_ref(),
            handles.window,
            handles.display,
        );
        let out = call(&mut ctx);
        frame.redraw_requested |= redraw_requested;
        out
    }
}

#[cfg(test)]
#[path = "frame_ctx_test.rs"]
mod tests;
