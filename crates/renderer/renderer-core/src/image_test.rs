use super::*;

fn opaque(pixels: &[[u8; 3]]) -> Vec<u8> {
    pixels
        .iter()
        .flat_map(|[r, g, b]| [*r, *g, *b, 255])
        .collect()
}

#[test]
fn the_same_image_built_twice_gets_the_same_id() {
    let once = ImageData::new(opaque(&[[10, 20, 30], [40, 50, 60]]), 2, 1);
    let again = ImageData::new(opaque(&[[10, 20, 30], [40, 50, 60]]), 2, 1);
    assert_eq!(once.id, again.id);
}

#[test]
fn different_pixels_get_different_ids() {
    let a = ImageData::new(opaque(&[[10, 20, 30], [40, 50, 60]]), 2, 1);
    let b = ImageData::new(opaque(&[[10, 20, 30], [40, 50, 61]]), 2, 1);
    assert_ne!(a.id, b.id);
}

// Same bytes, different shape: the dimensions have to be part of the address or a 4x1 would be served the 2x2's texture.
#[test]
fn the_same_bytes_at_different_dimensions_get_different_ids() {
    let wide = ImageData::new(
        opaque(&[[1, 2, 3], [4, 5, 6], [7, 8, 9], [10, 11, 12]]),
        4,
        1,
    );
    let square = ImageData::new(
        opaque(&[[1, 2, 3], [4, 5, 6], [7, 8, 9], [10, 11, 12]]),
        2,
        2,
    );
    assert_ne!(wide.id, square.id);
}

// `new` premultiplies and `from_premultiplied` does not, so addressing has to happen after that step or the two would disagree about an image they both finished identically.
#[test]
fn both_constructors_address_the_same_finished_image_alike() {
    let half_alpha = vec![200, 100, 50, 128];
    let mut premultiplied = half_alpha.clone();
    premultiply_rgba(&mut premultiplied);
    assert_eq!(
        ImageData::new(half_alpha, 1, 1).id,
        ImageData::from_premultiplied(premultiplied, 1, 1).id
    );
}
