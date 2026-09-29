use telar_project::{AssetKind, asset_kind_for_id};

use crate::Baker;

pub(crate) struct ImageBaker;

/// The target that shows a baked picture by fetching it: a page, which has an address to fetch from and a decoder of its own. A WASI guest is also `wasm32`, and has neither.
const BROWSER: &str = "all(target_arch = \"wasm32\", target_os = \"unknown\")";

impl Baker for ImageBaker {
    fn kind(&self) -> &'static AssetKind {
        asset_kind_for_id("image").expect("image is a registered asset kind")
    }

    /// The pixels everywhere but a browser, where the picture is a file beside the page instead: a module carries none of its weight, and the browser decodes it off the main thread. One line, because the artifact's cache reads an entry back by its line.
    fn bake(&self, bytes: &[u8]) -> Result<String, String> {
        let pixels = renderer_assets::bake_image_to_source(bytes).map_err(|e| e.to_string())?;
        let web = crate::web_image::web_image(bytes)?;
        let copies = web
            .copies
            .iter()
            .map(|(width, file)| format!("({width}, {:?})", file.path))
            .collect::<Vec<_>>()
            .join(", ");
        Ok(format!(
            "{{ #[cfg({BROWSER})] let image = ImageData::linked({:?}, {}, {}, &[{copies}]); #[cfg(not({BROWSER}))] let image = {pixels}; image }}",
            web.full.path, web.width, web.height
        ))
    }
}

#[cfg(test)]
#[path = "image_test.rs"]
mod tests;
