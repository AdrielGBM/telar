mod color;
mod command;
pub mod culling;
pub mod dirty;
mod draw_state;
mod error;
pub mod font_config;
mod hash;
mod image;
mod path;
mod preprocess;
mod renderer;
mod shadow;
mod style;
pub mod style_pool;

pub const BEZIER_CIRCLE_K: f32 = 0.552_284_8;

pub use color::Color;
pub use command::DrawCommand;
pub use culling::{FontMetrics, apply_matrix, extend_bounds};
#[doc(hidden)]
pub use dirty::ScrollBlit;
pub use draw_state::{DrawState, IDENTITY_MATRIX, compose_matrix, for_each_with_matrix};
pub use error::RendererError;
pub use font_config::FontConfig;
pub use hash::{hash_draw_commands, hash_draw_commands_into, hash_pod_slice};
pub use image::{ImageData, ImageFilter, premultiply_rgba};
pub use path::{PathData, PathVerb};
pub use preprocess::{blur_padding, blur_sigma, expand_fill_layers, scale_commands};
pub use renderer::RenderBackend;
pub use shadow::ShadowLayout;
pub use style::{
    BorderRadius, FillRule, Gradient, GradientKind, GradientStop, GradientStops, LineCap, LineJoin,
    Paint, PathStyle, RectStyle, Scale, Shadow, ShapeStyle, Stroke, TextStyle,
};
pub use style_pool::{hash_path_style, hash_rect_style, hash_text_style};
