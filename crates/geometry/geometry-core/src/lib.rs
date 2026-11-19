//! The plain geometric values every other crate speaks in: points, rects, sizes, colours, corner radii and affine transforms, and an index of rects by the bands of a page they cross.
//!
//! Deliberately dependency-free, so layout, rendering and the widget tree can all name the same types without any of them depending on each other.

#![warn(rustdoc::broken_intra_doc_links)]

mod bands;
mod border_radius;
mod box_model;
mod cell_size;
mod color;
mod grid;
mod insets;
mod object_fit;
mod point;
mod rect;
mod size;
mod transform;
mod wire;

pub use bands::Bands;
pub use border_radius::BorderRadius;
pub use box_model::BoxModel;
pub use cell_size::CellSize;
pub use color::{
    BODY_TEXT_CONTRAST, Color, ENHANCED_TEXT_CONTRAST, LARGE_TEXT_CONTRAST, NON_TEXT_CONTRAST,
    parse_hex,
};
pub use grid::{InvalidStep, LayoutGrid};
pub use insets::Insets;
pub use object_fit::{ObjectFit, fit_rect};
pub use point::Point;
pub use rect::Rect;
pub use size::Size;
pub use transform::Transform;
