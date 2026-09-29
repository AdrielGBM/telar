use super::*;

fn encoded(width: u32, height: u32, format: ImageFormat) -> Vec<u8> {
    let noise = image::RgbImage::from_fn(width, height, |x, y| {
        let v = (x.wrapping_mul(31) ^ y.wrapping_mul(17)) as u8;
        image::Rgb([v, v.wrapping_add(80), x as u8])
    });
    let mut bytes = Vec::new();
    DynamicImage::ImageRgb8(noise)
        .write_to(&mut Cursor::new(&mut bytes), format)
        .unwrap();
    bytes
}

#[test]
fn a_wide_picture_gets_halving_copies_down_to_the_narrowest() {
    let source = encoded(2000, 1000, ImageFormat::Jpeg);
    let web = web_image(&source).unwrap();
    assert_eq!((web.width, web.height), (2000, 1000));
    assert_eq!(web.full.extension, "jpg");
    assert_eq!(
        web.full.bytes, source,
        "a format a browser reads ships as it is"
    );
    let widths: Vec<u32> = web.copies.iter().map(|(width, _)| *width).collect();
    assert_eq!(widths, vec![500, 1000]);
    for (width, copy) in &web.copies {
        let decoded = image::load_from_memory(&copy.bytes).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (*width, width / 2));
        assert_eq!(copy.extension, "jpg");
    }
}

#[test]
fn a_narrow_picture_has_no_copies() {
    let web = web_image(&encoded(900, 300, ImageFormat::Png)).unwrap();
    assert!(web.copies.is_empty());
}

#[test]
fn a_format_no_browser_reads_ships_as_png() {
    let web = web_image(&encoded(8, 8, ImageFormat::Tiff)).unwrap();
    assert_eq!(web.full.extension, "png");
    assert_eq!(
        image::guess_format(&web.full.bytes).unwrap(),
        ImageFormat::Png
    );
}

#[test]
fn the_same_bytes_give_the_same_files() {
    let source = encoded(1200, 600, ImageFormat::Png);
    let (a, b) = (web_image(&source).unwrap(), web_image(&source).unwrap());
    assert_eq!(a.full, b.full);
    assert_eq!(a.copies, b.copies);
}
