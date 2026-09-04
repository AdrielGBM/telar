//! Telar's terminal platform: a window, an event loop and an input mapping over a terminal.
//!
//! Pairs with `telar-renderer-tui`, which draws the frames. The two agree on one number — how many logical pixels a cell stands for — and on nothing else: this crate knows nothing about how a frame is painted, and the renderer knows nothing about how input arrives.

mod clipboard;
mod map;
mod platform;
mod term;
mod window;

pub use clipboard::OscClipboard;
pub use platform::{TuiPlatform, TuiPlatformConfig};
pub use term::restore;
pub use window::TuiWindow;
