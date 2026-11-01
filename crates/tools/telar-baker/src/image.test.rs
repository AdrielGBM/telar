use super::*;

const DOT_PNG: &[u8] = include_bytes!("../fixtures/dot.png");

#[test]
fn bakes_the_pixels_for_every_target_but_a_browser() {
    let pixels = renderer_assets::bake_image_to_source(DOT_PNG).unwrap();
    let baked = ImageBaker.bake(DOT_PNG).unwrap();
    assert!(baked.contains(&format!("#[cfg(not({BROWSER}))] let image = {pixels};")));
    assert!(!baked.contains('\n'), "an entry is one line");
}

#[test]
fn bakes_the_address_for_a_browser() {
    let baked = ImageBaker.bake(DOT_PNG).unwrap();
    let web = crate::web_image::web_image(DOT_PNG).unwrap();
    assert!(baked.contains(&format!(
        "#[cfg({BROWSER})] let image = ImageData::linked({:?}, {}, {}, &[]);",
        web.full.path, web.width, web.height
    )));
}

#[test]
fn kind_id_is_image() {
    assert_eq!(ImageBaker.kind().id, "image");
}
