//! A first-class headless [`platform_core::Platform`] backend and the one canonical offscreen window marker.
//!
//! [`HeadlessWindow`] is a window with no window system behind it, satisfying both the renderer bound (raw-window-handle) and the platform bound ([`platform_core::Window`]).
//!
//! [`HeadlessPlatform`] drives a real app through the same [`platform_core::EventHandler`] seam as the winit backend, without a window system — a deterministic proving ground for the bring-your-own-`Platform` seam and an integration-test harness that can assert on read-back pixels.

#![warn(rustdoc::broken_intra_doc_links)]

mod platform;
mod seat;
mod window;

pub use platform::{FrameSink, HeadlessPlatform, SurfaceFrameSink};
pub use seat::{HeadlessSeat, PointerStep};
pub use window::{HeadlessWindow, TitleSink};
