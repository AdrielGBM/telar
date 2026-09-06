//! Runtime decoders and transports for Telar assets.
//!
//! `ui-core` owns the seam — [`AssetTransport`], [`AssetCache`], [`AssetDecoder`] — and ships no implementation of any of them, so the facade an application compiles has no opinion about which formats exist or where bytes come from. This crate is where those opinions live: a decoder per format, a transport per source, each behind its own feature, and none of them reachable unless asked for.
//!
//! Nothing here is privileged. An application that wants a format this crate does not carry, or bytes from a pack file, implements the same three traits against the same seam.

#![warn(rustdoc::broken_intra_doc_links)]

pub use ui_core::{AssetCache, AssetDecoder, AssetError, AssetKey, AssetTransport, Reply};
