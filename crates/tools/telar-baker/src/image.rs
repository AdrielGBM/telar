use telar_transpiler::{AssetKind, asset_kind_for_id};

use crate::Baker;

pub(crate) struct ImageBaker;

impl Baker for ImageBaker {
    fn kind(&self) -> &'static AssetKind {
        asset_kind_for_id("image").expect("image is a registered asset kind")
    }

    fn bake(&self, bytes: &[u8]) -> Result<String, String> {
        renderer_assets::bake_image_to_source(bytes).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
#[path = "image_test.rs"]
mod tests;
