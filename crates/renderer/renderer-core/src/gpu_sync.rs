//! Process-global lock serializing GPU/window surface *lifecycle* against active rendering.
//!
//! With M3 several hardware surfaces live in one process, each rendering on its own thread. On Wayland the `wl_surface` and the Vulkan swapchain are driven from different threads — the main thread owns the winit window, the render thread acquires/presents. Creating or destroying one window's surface on the main thread while another window's render thread is inside `vkAcquireNextImageKHR` corrupts the shared driver connection and segfaults (reproduced on the NVIDIA driver with two hardware surfaces). Render threads run concurrently under a shared *read* guard; a surface's creation or teardown takes the exclusive *write* guard, which waits for every in-flight frame to finish and blocks new ones — so lifecycle can never overlap a live acquire/present.

use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

static GPU_LIFECYCLE: RwLock<()> = RwLock::new(());

/// Held for the duration of a render thread's frame (acquire → present); concurrent with other render threads, but mutually exclusive with surface lifecycle. The `()` payload has no invariants, so a panic poisoning the lock is irrelevant — recover and continue.
pub fn render_guard() -> RwLockReadGuard<'static, ()> {
    GPU_LIFECYCLE.read().unwrap_or_else(|e| e.into_inner())
}

/// Held while a window/renderer surface is created or destroyed; exclusive against every render thread. The caller must not already hold a render guard on this thread (would self-deadlock).
pub fn lifecycle_guard() -> RwLockWriteGuard<'static, ()> {
    GPU_LIFECYCLE.write().unwrap_or_else(|e| e.into_inner())
}
