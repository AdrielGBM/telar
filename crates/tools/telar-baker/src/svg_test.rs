use super::*;

const ICON_SVG: &[u8] = include_bytes!("../fixtures/icon.svg");

#[test]
fn bakes_the_same_expression_as_the_current_path() {
    let expected = renderer_assets::bake_to_source(std::str::from_utf8(ICON_SVG).unwrap()).unwrap();
    let actual = SvgBaker.bake(ICON_SVG).unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn non_utf8_bytes_are_rejected_before_reaching_the_svg_parser() {
    let invalid = [0x53, 0x76, 0x67, 0xff, 0xfe];
    assert!(
        SvgBaker.bake(&invalid).is_err(),
        "non-utf8 bytes are rejected before the parser ever sees them"
    );
}

#[test]
fn kind_id_is_svg() {
    assert_eq!(SvgBaker.kind().id, "svg");
}
