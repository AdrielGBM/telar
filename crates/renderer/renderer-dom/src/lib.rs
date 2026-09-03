//! Telar's document renderer: a frame reconciled into real elements, laid out by CSS.
//!
//! The other backends are handed rects that are already positioned and fill them with pixels. This one is
//! handed the same stream and reads a different part of it: each box's *intent* — the flex and grid
//! declarations it asked layout for — and hands those to the browser, which positions it. Taffy still runs;
//! what it computes is what hit-testing, scrolling and anchored overlays read, and what a parity test
//! compares the browser's answer against.
//!
//! What this buys over drawing pixels is everything a document is and a canvas is not: text that can be
//! selected and found, elements a screen reader can walk, native focus, and an input method that works.
//!
//! Only the halves that touch the browser are compiled off `wasm32` — the ones that turn a style into CSS
//! and a shape into SVG are plain string building, and are tested on the host that builds them.

// The document half of the crate is absent off wasm, so what it would have called is unreachable there.
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

mod paint;
mod vector;
mod wrap;

#[cfg(target_arch = "wasm32")]
mod bitmap;
#[cfg(target_arch = "wasm32")]
mod entry;
#[cfg(target_arch = "wasm32")]
mod metrics;
#[cfg(target_arch = "wasm32")]
mod reconcile;
#[cfg(target_arch = "wasm32")]
mod renderer;

#[cfg(target_arch = "wasm32")]
pub use metrics::CanvasTextMetrics;
#[cfg(target_arch = "wasm32")]
pub use renderer::{DomRenderer, DomRendererFactory};
