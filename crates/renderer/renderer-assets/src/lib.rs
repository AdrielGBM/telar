//! External asset formats (SVG, PNG/JPEG) decoded and baked into the renderer's drawing vocabulary.
//!
//! `renderer-core` owns the vocabulary (`DrawCommand`, `PathData`, `ImageData`, …); this crate concentrates the usvg/resvg/image dependencies and turns those external formats into that vocabulary. `dynamic-svg`/`dynamic-image` drive the runtime parse/decode path; `bake` is the separate, host-only feature that pulls in the same deps (plus every format `image` ships) for the build-time bakers, which the transpiler enables so baking works without adding usvg/image to the app runtime.

#![warn(rustdoc::broken_intra_doc_links)]

mod image;
mod svg;
#[cfg(feature = "dynamic-svg")]
mod svg_cache;

#[cfg(any(feature = "dynamic-image", feature = "bake"))]
pub use image::ImageError;
#[cfg(feature = "bake")]
pub use image::bake_image_to_source;
#[cfg(feature = "dynamic-image")]
pub use image::decode;
#[cfg(feature = "bake")]
pub use svg::bake_to_source;
pub use svg::{SvgData, SvgError, VectorCommand};
#[cfg(feature = "dynamic-svg")]
pub use svg_cache::{static_key, svg_cached};
