//! One pipeline per primitive, plus the vertex and instance formats they share.

pub(crate) mod image;
pub(crate) mod layer;
pub(crate) mod line;
pub(crate) mod path;
pub(crate) mod rect;
pub(crate) mod text;

mod fill;
mod instance_pipeline;
mod pipeline;
mod viewport;

pub(crate) use fill::{EncodedFill, encode_fill_style};
pub(crate) use instance_pipeline::{InstancePipeline, upload_instances};
pub(crate) use pipeline::create_render_pipeline;
pub(crate) use viewport::{Viewport, create_viewport_bind_group_layout, create_viewport_buffer};
