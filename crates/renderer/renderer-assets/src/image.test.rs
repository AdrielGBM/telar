use super::*;

#[test]
fn a_tint_replaces_the_colour_and_scales_the_coverage() {
    let mut pixels = [
        255, 255, 255, 255, // opaque
        128, 128, 128, 128, // half coverage, premultiplied
        0, 0, 0, 0, // transparent
    ];
    apply_tint_premultiplied(&mut pixels, Color::rgba(1.0, 0.5, 0.0, 1.0));
    assert_eq!(
        pixels,
        [
            255, 128, 0, 255, //
            128, 64, 0, 128, //
            0, 0, 0, 0,
        ]
    );
}

#[test]
fn a_tint_alpha_scales_the_coverage_and_stays_premultiplied() {
    let mut pixels = [255, 255, 255, 255, 128, 128, 128, 128];
    apply_tint_premultiplied(&mut pixels, Color::rgba(0.0, 0.0, 1.0, 0.5));
    assert_eq!(pixels, [0, 0, 128, 128, 0, 0, 64, 64]);
}

#[cfg(feature = "bake")]
mod baked_formats {
    use super::super::*;

    // The baker takes every format `image` ships, since a build-time failure over a missing decoder would break a valid file; GIF stands in for the set, which only decodes because `bake` asks for all of them.
    #[test]
    fn the_baker_reads_a_format_no_runtime_knob_would_have_turned_on() {
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
            .expect("the gif encoder comes with `bake`");

        let (rgba, w, h) = decode_rgba8(&bytes).expect("gif decodes for the baker");
        assert_eq!((w, h), (2, 3));
        assert_eq!(rgba, [10, 20, 30, 255].repeat(2 * 3));
    }
}
