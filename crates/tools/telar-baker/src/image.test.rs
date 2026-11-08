use super::*;

const DOT_PNG: &[u8] = include_bytes!("../fixtures/dot.png");

fn rgba_of(expression: &str) -> Vec<u8> {
    let literal = expression
        .strip_prefix("ImageData::new(b\"")
        .and_then(|rest| rest.split_once("\".to_vec()"))
        .expect("the baked pixels are a byte-string literal")
        .0;
    literal
        .split("\\x")
        .skip(1)
        .map(|byte| u8::from_str_radix(byte, 16).expect("every byte is a two-digit escape"))
        .collect()
}

#[test]
fn bakes_the_png_into_its_decoded_straight_rgba_for_every_target_but_a_browser() {
    let baked = ImageBaker.bake(DOT_PNG).unwrap();
    assert!(!baked.contains('\n'), "an entry is one line");
    let marker = format!("#[cfg(not({BROWSER}))] let image = ");
    let expression = baked
        .split_once(&marker)
        .and_then(|(_, rest)| rest.split_once("; image }"))
        .expect("the pixels are the branch for every target but a browser")
        .0;
    assert!(expression.ends_with(".to_vec(), 40, 40)"), "{expression}");

    let rgba = rgba_of(expression);
    assert_eq!(rgba.len(), 40 * 40 * 4);
    let pixel = |x: usize, y: usize| rgba[(y * 40 + x) * 4..][..4].to_vec();
    assert_eq!(
        pixel(0, 0),
        [239, 71, 111, 0],
        "the corner is clear, its colour kept straight"
    );
    assert_eq!(
        pixel(20, 20),
        [254, 204, 102, 255],
        "the middle of the dot is opaque"
    );
    let opaque = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|px| px[3] == 255)
        .count();
    assert_eq!(opaque, 1125, "the dot's coverage");
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
