use super::*;

fn opaque(pixels: &[[u8; 3]]) -> Vec<u8> {
    pixels
        .iter()
        .flat_map(|[r, g, b]| [*r, *g, *b, 255])
        .collect()
}

#[test]
fn the_same_image_built_twice_gets_the_same_id() {
    let once = ImageData::new(opaque(&[[10, 20, 30], [40, 50, 60]]), 2, 1)
        .expect("the pixels fill the image");
    let again = ImageData::new(opaque(&[[10, 20, 30], [40, 50, 60]]), 2, 1)
        .expect("the pixels fill the image");
    assert_eq!(once.id(), again.id());
}

#[test]
fn different_pixels_get_different_ids() {
    let a = ImageData::new(opaque(&[[10, 20, 30], [40, 50, 60]]), 2, 1)
        .expect("the pixels fill the image");
    let b = ImageData::new(opaque(&[[10, 20, 30], [40, 50, 61]]), 2, 1)
        .expect("the pixels fill the image");
    assert_ne!(a.id(), b.id());
}

// Same bytes, different shape: the dimensions have to be part of the address or a 4x1 would be served the 2x2's texture.
#[test]
fn the_same_bytes_at_different_dimensions_get_different_ids() {
    let wide = ImageData::new(
        opaque(&[[1, 2, 3], [4, 5, 6], [7, 8, 9], [10, 11, 12]]),
        4,
        1,
    )
    .expect("the pixels fill the image");
    let square = ImageData::new(
        opaque(&[[1, 2, 3], [4, 5, 6], [7, 8, 9], [10, 11, 12]]),
        2,
        2,
    )
    .expect("the pixels fill the image");
    assert_ne!(wide.id(), square.id());
}

#[test]
fn pixels_that_do_not_fill_the_image_are_refused() {
    let short = ImageData::new(vec![0; 4 * 3], 2, 2).map(|image| image.id());
    assert_eq!(
        short,
        Err(ImageDataError {
            width: 2,
            height: 2,
            bytes: 12
        })
    );
    assert!(ImageData::from_premultiplied(vec![0; 4 * 5], 2, 2).is_err());
    assert!(ImageData::new(Vec::new(), u32::MAX, u32::MAX).is_err());
}

// `new` premultiplies and `from_premultiplied` does not, so addressing has to happen after that step or the two would disagree about an image they both finished identically.
#[test]
fn both_constructors_address_the_same_finished_image_alike() {
    let half_alpha = vec![200, 100, 50, 128];
    let mut premultiplied = half_alpha.clone();
    premultiply_rgba(&mut premultiplied);
    assert_eq!(
        ImageData::new(half_alpha, 1, 1)
            .expect("the pixels fill the image")
            .id(),
        ImageData::from_premultiplied(premultiplied, 1, 1)
            .expect("the pixels fill the image")
            .id()
    );
}

#[test]
fn premultiplying_rounds_to_the_nearest_value_and_leaves_opaque_pixels_alone() {
    for pixel_count in [1, 4, 7] {
        for alpha in 0..=255u32 {
            for channel in 0..=255u32 {
                let mut pixels =
                    [channel as u8, channel as u8, channel as u8, alpha as u8].repeat(pixel_count);
                premultiply_rgba(&mut pixels);
                let expected = (channel * alpha + 127) / 255;
                for pixel in pixels.as_chunks::<4>().0 {
                    assert_eq!(
                        u32::from(pixel[0]),
                        expected,
                        "channel {channel} at alpha {alpha}, {pixel_count} pixels"
                    );
                    assert_eq!(u32::from(pixel[3]), alpha);
                }
            }
        }
    }
}
