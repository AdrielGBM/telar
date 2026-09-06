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
mod tests {
    use super::*;

    const ICON_SVG: &[u8] = include_bytes!("../tests/fixtures/icon.svg");

    #[test]
    fn bakes_the_same_expression_as_the_current_path() {
        let expected =
            renderer_assets::bake_to_source(std::str::from_utf8(ICON_SVG).unwrap()).unwrap();
        let actual = SvgBaker.bake(ICON_SVG).unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn non_utf8_bytes_are_rejected_before_reaching_the_svg_parser() {
        let invalid = [0x53, 0x76, 0x67, 0xff, 0xfe];
        assert!(SvgBaker.bake(&invalid).is_err());
    }

    #[test]
    fn kind_id_is_svg() {
        assert_eq!(SvgBaker.kind().id, "svg");
    }
}
