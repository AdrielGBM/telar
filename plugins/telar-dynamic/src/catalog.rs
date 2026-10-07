//! Parsing one locale's TOML into the catalog `i18n_core::translate` reads.

use i18n_core::Catalog;
use ui_core::{AssetDecoder, AssetError};

/// Decodes a locale's TOML into the `&'static Catalog` [`i18n_core::set_catalog`] installs.
///
/// This is the half of i18n that arrives at run time: a downloaded or user-supplied language pack goes through the same transport → cache → decode path an icon does, which is what puts a corrupt TOML on the failure branch instead of into the cache. A catalog the transpiler baked needs none of this and links no parser.
///
/// One decoder per locale, and the locale is not the asset id: [`AssetDecoder::decode`] is handed bytes without the key that named them, while [`Catalog::from_toml`] has to stamp the messages with the tag they are for. An application switching languages builds a decoder per language, sharing one transport and one cache between them.
///
/// Every successful decode leaks its catalog, because a `&'static Catalog` is what the lookup path takes. That is once per locale in the shape this is for; decoding the same locale in a loop leaks once per pass.
pub struct CatalogDecoder {
    locale: String,
}

impl CatalogDecoder {
    /// A decoder for the locale `locale` names — the BCP 47 tag the messages are written in, and the one they will be found under.
    pub fn new(locale: impl Into<String>) -> Self {
        Self {
            locale: locale.into(),
        }
    }
}

impl AssetDecoder for CatalogDecoder {
    fn kind(&self) -> &'static str {
        "catalog"
    }

    type Output = &'static Catalog;

    fn decode(&self, bytes: &[u8]) -> Result<Self::Output, AssetError> {
        let text = std::str::from_utf8(bytes)
            .map_err(|e| AssetError(format!("catalog is not valid utf-8: {e}")))?;
        Catalog::from_toml(&self.locale, text).map_err(AssetError)
    }
}

#[cfg(test)]
#[path = "catalog_test.rs"]
mod tests;
