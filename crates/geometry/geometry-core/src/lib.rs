//! The plain geometric values every other crate speaks in: points, rects, colours, corner radii and affine transforms.
//!
//! Deliberately dependency-free, so layout, rendering and the widget tree can all name the same types without any of them depending on each other.

#![warn(rustdoc::broken_intra_doc_links)]

mod border_radius;
mod color;
mod grid;
mod object_fit;
mod point;
mod rect;
mod transform;

pub use border_radius::BorderRadius;
pub use color::Color;
pub use grid::{LayoutGrid, layout_grid, set_layout_grid};
pub use object_fit::{ObjectFit, fit_rect};
pub use point::Point;
pub use rect::Rect;
pub use transform::Transform;
