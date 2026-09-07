//! That an application can name everything this crate hands it without depending on a crate behind the facade.
//!
//! Every function below writes down a decoder's output type using `telar_dynamic::` paths and nothing else. They are compile-time assertions with a `#[test]` attached so `cargo test` reports them: if a re-export goes missing, this file stops compiling, which is the failure that matters — a decoder whose result cannot be written down without reaching past the facade for `renderer-assets` or `renderer-core` is a decoder an application has to take a second dependency to use, and that second dependency is how two copies of `SvgData` get resolved.

use super::*;
use std::sync::Arc;

#[cfg(feature = "svg")]
#[test]
fn an_svg_decodes_into_a_type_this_crate_names() {
    fn _output(decoder: &SvgDecoder, bytes: &[u8]) -> Option<Arc<SvgData>> {
        decoder.decode(bytes).ok()
    }
}

#[cfg(feature = "image")]
#[test]
fn a_bitmap_decodes_into_a_type_this_crate_names() {
    fn _output(decoder: &ImageDecoder, bytes: &[u8]) -> Option<Arc<ImageData>> {
        decoder.decode(bytes).ok()
    }
}

#[cfg(feature = "catalog")]
#[test]
fn a_catalog_decodes_into_a_type_this_crate_names() {
    fn _output(decoder: &CatalogDecoder, bytes: &[u8]) -> Option<&'static Catalog> {
        decoder.decode(bytes).ok()
    }
}

/// The two caches answer the same role, so an application can hold either behind one `Arc` and decide per target which it installs.
#[test]
fn both_caches_are_the_same_role() {
    let _memory: Arc<dyn AssetCache> = Arc::new(MemoryCache::default());
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _disk: Arc<dyn AssetCache> = Arc::new(DiskCache::new("/tmp/telar-assets"));
    }
}
