//! Runtime decoders and transports for Telar assets.
//!
//! `ui-core` owns the seam — [`AssetTransport`], [`AssetCache`], [`AssetDecoder`] — and ships no implementation of any of them, so the facade an application compiles has no opinion about which formats exist or where bytes come from. This crate is where those opinions live: a decoder per format, a transport per source, each behind its own feature, and none of them reachable unless asked for.
//!
//! Nothing here is privileged. An application that wants a format this crate does not carry, or bytes from a pack file, implements the same three traits against the same seam.
//!
//! **Keep this crate on the same version as `telar`.** Both depend on `renderer-assets` for `SvgData`, so a mismatch resolves two copies of it and the `Arc<SvgData>` a decoder here produces stops being the type the widget over there accepts — a type error naming one struct twice. It is the same lockstep `telar` and `telar-macros` already have, and for the same reason.
//!
//! Be clear about what the split does and does not buy. It costs the same to compile: enabling `telar-dynamic/svg-text` links exactly what `telar/svg-text` used to. What it buys is that `telar` no longer carries twenty knobs about formats most applications never touch, and that a third-party decoder arrives through the same door as ours.
//!
//! ```ignore
//! use std::sync::Arc;
//! use telar::AssetLoader;
//! use telar_dynamic::{DiskCache, HttpTransport, SvgDecoder};
//!
//! let icons = AssetLoader::new(
//!     Arc::new(HttpTransport::new("https://api.iconify.design/{name}.svg")),
//!     Some(Arc::new(DiskCache::new(
//!         telar::paths::cache().unwrap_or_else(std::env::temp_dir).join("icons"),
//!     ))),
//!     Arc::new(SvgDecoder),
//! );
//! // In a view: `Loading` until it lands, then the parsed SVG, re-rendering whoever read it.
//! match icons.get("mdi/home").get() { /* .. */ }
//! ```
//!
//! # Feature flags
#![cfg_attr(
    feature = "document-features",
    doc = document_features::document_features!()
)]
#![warn(rustdoc::broken_intra_doc_links)]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]

#[cfg(feature = "catalog")]
mod catalog;
#[cfg(feature = "dir")]
mod dir;
mod disk_cache;
#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
mod http;
#[cfg(feature = "image")]
mod image;
#[cfg(feature = "svg")]
mod svg;

#[cfg(feature = "catalog")]
pub use catalog::CatalogDecoder;
#[cfg(feature = "dir")]
pub use dir::DirTransport;
pub use disk_cache::DiskCache;
#[cfg(all(feature = "http", not(target_arch = "wasm32")))]
pub use http::HttpTransport;
#[cfg(feature = "image")]
pub use image::ImageDecoder;
#[cfg(feature = "svg")]
pub use svg::SvgDecoder;

pub use ui_core::{AssetCache, AssetDecoder, AssetError, AssetKey, AssetTransport, Reply};
