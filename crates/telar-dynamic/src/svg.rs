//! Parsing an SVG document into the shapes a widget draws.

use std::sync::Arc;

use renderer_assets::SvgData;
use ui_core::{AssetDecoder, AssetError};

/// Decodes an SVG document into the `Arc<SvgData>` the `Svg` widget takes.
///
/// Which parts of the document survive is a build decision, and it fails quietly by design: `<text>` needs `svg-text` and is dropped without it, because which features an SVG uses is not knowable until the string arrives. An SVG baked at build time keeps everything either way — the baker runs on the host with all of it on.
pub struct SvgDecoder;

impl AssetDecoder for SvgDecoder {
    fn kind(&self) -> &'static str {
        "svg"
    }

    type Output = Arc<SvgData>;

    fn decode(&self, bytes: &[u8]) -> Result<Self::Output, AssetError> {
        let text = std::str::from_utf8(bytes)
            .map_err(|e| AssetError(format!("svg is not valid utf-8: {e}")))?;
        SvgData::from_str(text)
            .map(Arc::new)
            .map_err(|e| AssetError(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SQUARE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M2 2 H22 V22 H2 Z"/></svg>"#;

    #[test]
    fn a_document_decodes_and_a_body_that_is_not_one_does_not() {
        assert!(SvgDecoder.decode(SQUARE.as_bytes()).is_ok());
        assert!(SvgDecoder.decode(b"<html>404 Not Found</html>").is_err());
    }

    /// The kind namespaces the cache, so an SVG named `logo` and a bitmap named `logo` cannot share a file.
    #[test]
    fn the_kind_names_the_format_and_not_the_id() {
        assert_eq!(SvgDecoder.kind(), "svg");
    }
}
