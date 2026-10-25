use std::ptr::NonNull;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use raw_window_handle::{WaylandDisplayHandle, WaylandWindowHandle};

use super::*;
use crate::RedrawWaker;

fn wayland() -> (RawWindowHandle, RawDisplayHandle) {
    let surface = NonNull::<u8>::dangling().cast();
    let display = NonNull::<u64>::dangling().cast();
    (
        RawWindowHandle::Wayland(WaylandWindowHandle::new(surface)),
        RawDisplayHandle::Wayland(WaylandDisplayHandle::new(display)),
    )
}

/// The guest's frame wakes the host's loop through a waker that keeps a panic on the host's side, sees the host's window, and asks for a redraw on the host's context.
#[test]
fn a_guest_frame_wakes_the_host_sees_its_window_and_carries_its_redraw_back() {
    let woken = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&woken);
    let waker = RedrawWaker::new(move || {
        flag.store(true, Ordering::SeqCst);
        panic!("the loop is gone");
    });
    let (window, display) = wayland();
    let mut redraw_requested = false;
    let mut ctx = AppCtx::new(
        &mut redraw_requested,
        Some(&waker),
        Some(window),
        Some(display),
    );

    FrameCtx::lend(&mut ctx, |frame| unsafe {
        FrameCtx::enter(frame, |guest_ctx| {
            assert_eq!(guest_ctx.raw_window_handle(), Some(window));
            assert_eq!(guest_ctx.raw_display_handle(), Some(display));
            guest_ctx
                .redraw_waker()
                .expect("the guest gets a waker")
                .wake();
        })
    });
    assert!(woken.load(Ordering::SeqCst));
    assert!(!redraw_requested, "nothing asked for one yet");

    let mut ctx = AppCtx::new(&mut redraw_requested, None, None, None);
    FrameCtx::lend(&mut ctx, |frame| unsafe {
        FrameCtx::enter(frame, |guest_ctx| {
            assert!(guest_ctx.redraw_waker().is_none());
            assert_eq!(guest_ctx.raw_window_handle(), None);
            guest_ctx.request_redraw();
        })
    });
    assert!(redraw_requested);
}

#[test]
fn a_waker_the_guest_keeps_outlives_the_frame_it_came_with() {
    let woken = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&woken);
    let waker = RedrawWaker::new(move || flag.store(true, Ordering::SeqCst));
    let mut redraw_requested = false;
    let mut ctx = AppCtx::new(&mut redraw_requested, Some(&waker), None, None);
    let kept = FrameCtx::lend(&mut ctx, |frame| unsafe {
        FrameCtx::enter(frame, |guest_ctx| guest_ctx.redraw_waker())
    })
    .expect("the guest gets a waker");
    drop(waker);
    kept.wake();
    assert!(woken.load(Ordering::SeqCst));
}
