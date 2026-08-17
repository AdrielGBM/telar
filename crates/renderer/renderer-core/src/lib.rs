mod command;
pub mod culling;
pub mod dirty;
mod draw_state;
mod error;
pub mod font_config;
pub mod gpu_sync;
mod hash;
mod image;
mod metrics;
mod path;
pub mod perf;
mod preprocess;
mod renderer;
mod shadow;
mod style;
mod style_pool;

pub const BEZIER_CIRCLE_K: f32 = 0.552_284_8;

pub use command::{DrawCommand, TextRun};
pub use culling::{FontMetrics, extend_bounds};
#[doc(hidden)]
pub use dirty::ScrollBlit;
pub use draw_state::{DrawState, for_each_with_matrix, transform_clip_rect};
pub use error::RendererError;
pub use font_config::FontConfig;
pub use geometry_core::{BorderRadius, Color};
pub use hash::{hash_draw_commands, hash_draw_commands_into, hash_pod_slice};
pub use image::{ExternalTexture, ImageData, ImageFilter, premultiply_rgba};
pub use metrics::{
    TextMetrics, line_height, measure_ink_bounds, measure_rich_text, measure_text,
    set_default_text_metrics, set_text_metrics,
};
pub use path::{PathData, PathVerb};
pub use preprocess::{ScaleScratch, blur_padding, blur_sigma, expand_fill_layers};
pub use renderer::RenderBackend;
pub use shadow::ShadowLayout;
pub use style::{
    BorderWidths, FillRule, GlyphRaster, Gradient, GradientKind, GradientStop, GradientStops,
    LineCap, LineJoin, Paint, PathStyle, RectStyle, Scale, Shadow, ShapeStyle, Stroke, TextAlign,
    TextStyle, border_inner_shape,
};
pub use style_pool::hash_path_style;
