//! External asset formats (SVG, PNG/JPEG) decoded and baked into the renderer's drawing vocabulary.
//!
//! `renderer-core` owns the vocabulary (`DrawCommand`, `PathData`, `ImageData`, …); this crate concentrates the usvg/resvg/image dependencies and turns those external formats into that vocabulary. The `dynamic-svg`/`dynamic-image` features carry those deps and drive both the runtime parse/decode AND the build-time bakers; the transpiler enables them host-side so baking is always available without adding usvg/image to the app runtime.

mod image;
mod svg;
#[cfg(feature = "dynamic-svg")]
mod svg_cache;

#[cfg(feature = "dynamic-image")]
pub use image::{ImageError, bake_image_to_source, decode};
#[cfg(feature = "dynamic-svg")]
pub use svg::bake_to_source;
pub use svg::{SvgData, SvgError, VectorCommand};
#[cfg(feature = "dynamic-svg")]
pub use svg_cache::{static_key, svg_cached};
