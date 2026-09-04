//! Telar's browser platform: a surface, input, and the animation-frame loop that drives them.
//!
//! Compiles to nothing off `wasm32`, so the crate can sit in the workspace and be checked by a host build without pulling `web-sys` into one.
//!
//! What it deliberately does *not* do is decide how a frame is drawn. It hands a host element to whichever renderer was installed — a canvas the GPU presents into, or the DOM itself — exactly as the terminal and desktop platforms hand over a window.

#![cfg(target_arch = "wasm32")]
#![warn(rustdoc::broken_intra_doc_links)]

mod clipboard;
mod dom;
mod log;
mod map;
mod platform;
mod window;

pub use clipboard::WebClipboard;
pub use dom::{host as host_element, page_setting};
pub use log::install_console_logging;
pub use platform::{WebPlatform, WebPlatformConfig};
pub use window::{Measured, WebWindow};
