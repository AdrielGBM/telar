use telar_transpiler::{AssetKind, asset_kind_for_id};

use crate::Baker;

pub(crate) struct SvgBaker;

impl Baker for SvgBaker {
    fn kind(&self) -> &'static AssetKind {
        asset_kind_for_id("svg").expect("svg is a registered asset kind")
    }

    fn bake(&self, bytes: &[u8]) -> Result<String, String> {
        let content =
            std::str::from_utf8(bytes).map_err(|e| format!("SVG asset is not valid UTF-8: {e}"))?;
        renderer_assets::bake_to_source(content).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
#[path = "svg_test.rs"]
mod tests;
