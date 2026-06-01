mod color;
mod command;
pub mod culling;
pub mod dirty;
mod draw_state;
mod error;
mod image;
mod path;
mod preprocess;
mod renderer;
mod style;

pub use color::Color;
pub use command::{DrawCommand, PathPayload, RectPayload, TextPayload};
pub use culling::union_rects;
pub use dirty::ScrollBlit;
pub use draw_state::{DrawState, IDENTITY_MATRIX, compose_matrix};
pub use error::RendererError;
pub use image::{ImageData, ImageFilter, premultiply_rgba};
pub use path::{PathData, PathVerb};
pub use preprocess::{blur_sigma, expand_fill_layers, scale_commands};
pub use renderer::RenderBackend;
pub use style::{
    BorderRadius, FillRule, Gradient, GradientKind, GradientStop, GradientStops, LineCap, LineJoin,
    LineStyle, Paint, PathStyle, RectStyle, Shadow, Stroke, TextStyle,
};
