mod color;
mod command;
pub mod culling;
pub mod dirty;
mod draw_state;
mod error;
pub mod font_config;
mod geometry;
mod image;
mod path;
mod preprocess;
mod renderer;
mod style;
pub mod style_pool;

pub const BEZIER_CIRCLE_K: f32 = 0.552_284_8;

pub use color::Color;
pub use command::DrawCommand;
pub use culling::{apply_matrix, union_rects};
#[doc(hidden)]
pub use dirty::ScrollBlit;
pub use draw_state::{DrawState, IDENTITY_MATRIX, compose_matrix};
pub use error::RendererError;
pub use font_config::FontConfig;
pub use image::{ImageData, ImageFilter, premultiply_rgba};
pub use path::{PathData, PathVerb};
pub use preprocess::{blur_padding, blur_sigma, expand_fill_layers, scale_commands};
pub use renderer::RenderBackend;
pub use style::{
    BorderRadius, FillRule, Gradient, GradientKind, GradientStop, GradientStops, LineCap, LineJoin,
    LineStyle, Paint, PathStyle, RectStyle, Shadow, Stroke, TextStyle,
};
pub use style_pool::{FRAME_STYLE_POOL, FrameStylePool, StyleHandle};
