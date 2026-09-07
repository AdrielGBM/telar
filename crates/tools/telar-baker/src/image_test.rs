use super::*;

const DOT_PNG: &[u8] = include_bytes!("../fixtures/dot.png");

#[test]
fn bakes_the_same_expression_as_the_current_path() {
    let expected = renderer_assets::bake_image_to_source(DOT_PNG).unwrap();
    let actual = ImageBaker.bake(DOT_PNG).unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn kind_id_is_image() {
    assert_eq!(ImageBaker.kind().id, "image");
}
