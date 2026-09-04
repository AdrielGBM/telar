//! Flexbox and grid layout over a taffy tree, addressed by node id.
//!
//! Styles are written logically — `start`/`end` rather than left/right — and resolved against the active [`Direction`] when the tree is computed, so one build serves LTR and RTL.

mod css;
mod direction;
mod engine;
mod error;
mod style;
mod track;

pub use css::Css;
pub use direction::Direction;
pub use engine::{LayoutEngine, MeasureFn, NodeId};
pub use error::LayoutError;
pub use style::{AlignItems, AvailableSpace, JustifyContent, LayoutStyle, Margin, SizeDimension};
pub use track::TemplateTrack;
