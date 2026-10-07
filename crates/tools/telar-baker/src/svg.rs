use telar_project::{AssetKind, asset_kind_for_id};

use crate::Baker;

pub(crate) struct SvgBaker;

impl Baker for SvgBaker {
    fn kind(&self) -> &'static AssetKind {
        asset_kind_for_id("svg").expect("svg is a registered asset kind")
    }

    fn bake(&self, bytes: &[u8]) -> Result<String, String> {
        bake_svg(bytes)
    }
}

/// An icon is an SVG document by the time it reaches a baker: whichever source resolved its id, from an Iconify set or a file, handed back standalone SVG.
pub(crate) struct IconBaker;

impl Baker for IconBaker {
    fn kind(&self) -> &'static AssetKind {
        asset_kind_for_id("icon").expect("icon is a registered asset kind")
    }

    fn bake(&self, bytes: &[u8]) -> Result<String, String> {
        bake_svg(bytes)
    }
}

fn bake_svg(bytes: &[u8]) -> Result<String, String> {
    let content =
        std::str::from_utf8(bytes).map_err(|e| format!("SVG asset is not valid UTF-8: {e}"))?;
    renderer_assets::bake_to_source(content).map_err(|e| e.to_string())
}

#[cfg(test)]
#[path = "svg_test.rs"]
mod tests;
