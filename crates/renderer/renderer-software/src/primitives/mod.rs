//! One rasterizer per primitive.

pub(crate) mod colr;
pub(crate) mod image;
pub(crate) mod line;
pub(crate) mod path;
pub(crate) mod rect;
pub(crate) mod text;

mod paint;
mod shadow;

pub(crate) use paint::*;
pub(crate) use shadow::*;
