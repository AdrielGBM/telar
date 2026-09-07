use super::*;

#[test]
fn a_body_that_is_not_a_bitmap_fails_rather_than_producing_an_empty_raster() {
    assert!(
        ImageDecoder.decode(b"<html>404 Not Found</html>").is_err(),
        "an error page is not a bitmap"
    );
}

/// The format knobs, checked where it matters: that turning one on actually reaches `image`.
///
/// A feature that forwards nowhere looks identical to one that works — [`ImageDecoder`] keeps compiling either way, and the difference only shows as a decode failure on a user's file. GIF stands in for all thirteen: it is behind `image-gif` exactly as the others are behind theirs, and `image` can encode it, so the test can make its own input instead of carrying hand-written bytes for a format nobody can read in review.
#[cfg(feature = "image-gif")]
#[test]
fn a_format_behind_a_feature_decodes_once_that_feature_is_on() {
    let source = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        2,
        3,
        image::Rgba([10, 20, 30, 255]),
    ));
    let mut bytes = Vec::new();
    source
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Gif,
        )
        .expect("the gif encoder is part of the same feature");

    let decoded = ImageDecoder
        .decode(&bytes)
        .expect("gif decodes with `image-gif` on");
    assert_eq!((decoded.width, decoded.height), (2, 3));
}
