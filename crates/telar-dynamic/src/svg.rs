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
#[path = "svg_test.rs"]
mod tests;
