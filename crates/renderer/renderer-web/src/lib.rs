//! Telar's browser renderer: a canvas surface and the wgpu backend that draws into it.
//!
//! Compiles to nothing off `wasm32`, so the crate sits in the workspace without pulling `web-sys` into a
//! host build.
//!
//! The device comes up asynchronously, because in a browser it can come up no other way: `requestAdapter`
//! and `requestDevice` are promises that only settle once the calling stack has returned. So the renderer
//! this hands back is real from the first frame and *empty* until the device lands — it drops the frames in
//! between and asks for another once it can draw. That is the same shape the runtime already has for the
//! desktop's background GPU build, which is why nothing above had to learn a new state.

#![cfg(target_arch = "wasm32")]

mod canvas;
mod renderer;

pub use canvas::{CanvasSurface, canvas_in};
pub use renderer::{WebGpuRenderer, WebGpuRendererFactory};
