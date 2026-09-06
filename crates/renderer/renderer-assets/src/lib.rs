//! External asset formats (SVG, PNG/JPEG) decoded and baked into the renderer's drawing vocabulary.
//!
//! `renderer-core` owns the vocabulary (`DrawCommand`, `PathData`, `ImageData`, …); this crate concentrates the usvg/resvg/image dependencies and turns those external formats into that vocabulary. `dynamic-svg` drives the runtime parse path; `bake` is the separate, host-only feature that pulls in the same deps (plus every format `image` ships) for the build-time bakers, which the transpiler enables so baking works without adding usvg/image to the app runtime.
//!
//! Decoding a bitmap at runtime is not here: it is `telar-dynamic`'s `ImageDecoder`, which calls `image` directly and carries the per-format knobs, so the only thing in this crate that reaches for a bitmap decoder is the baker.

#![warn(rustdoc::broken_intra_doc_links)]

mod image;
mod svg;

#[cfg(feature = "bake")]
pub use image::ImageError;
#[cfg(feature = "bake")]
pub use image::bake_image_to_source;
#[cfg(feature = "bake")]
pub use svg::bake_to_source;
pub use svg::{SvgData, SvgError, VectorCommand};
