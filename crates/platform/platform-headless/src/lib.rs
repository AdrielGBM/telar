//! A first-class headless [`platform_core::Platform`] backend and the one canonical offscreen window marker.
//!
//! [`HeadlessWindow`] replaces the ad-hoc markers that used to be duplicated across the renderer crates (the `HeadlessWindow` in `renderer-hardware` and a per-test `struct Fake;` in each renderer-software test). One type now satisfies both the renderer bound (raw-window-handle) and the platform bound ([`platform_core::Window`]).
//!
//! [`HeadlessPlatform`] drives a real app through the same [`platform_core::EventHandler`] seam as the winit backend, without a window system — a deterministic proving ground for the bring-your-own-`Platform` seam and an integration-test harness that can assert on read-back pixels.

mod platform;
mod window;

pub use platform::{FrameSink, HeadlessPlatform, SurfaceFrameSink};
pub use window::HeadlessWindow;
