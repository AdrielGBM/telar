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
//! Compiles to nothing off `wasm32`, so the crate sits in the workspace without pulling `web-sys` into a
//! host build.

#![cfg(target_arch = "wasm32")]

mod metrics;
mod paint;
mod reconcile;
mod renderer;

pub use metrics::CanvasTextMetrics;
pub use renderer::{DomRenderer, DomRendererFactory};
