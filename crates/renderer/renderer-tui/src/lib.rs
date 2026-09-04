//! Telar's terminal backend: composed draw commands in, a character-cell grid out.
//!
//! The terminal is not a raster surface, so this is not a rasteriser with a coarse pixel. It reads the same [`DrawCommand`](renderer_core::DrawCommand) stream every other backend reads and answers the question each command poses in the terminal's own vocabulary — a filled box is a run of coloured cells, a border is box-drawing, a paragraph is the glyphs themselves.
//!
//! The unit bridge is [`CellSize`]: the terminal reports its size in logical pixels (columns × a declared cell width), so layout runs exactly as it does on the desktop and nothing above this crate learns that a cell exists. See [`CellMetrics`] for why measurement is the piece that has to know.

#![warn(rustdoc::broken_intra_doc_links)]

mod buffer;
mod cell;
mod color;
mod metrics;
mod paint;
mod renderer;
mod wrap;

pub use buffer::CellBuffer;
pub use cell::{Attrs, Cell, Grapheme};
pub use color::{ColorDepth, Rgb};
pub use metrics::{CellMetrics, CellSize};
pub use paint::{CellRect, Painter};
pub use renderer::{TuiConfig, TuiRenderer, TuiRendererFactory};
pub use wrap::{WrapConfig, WrappedLine};
