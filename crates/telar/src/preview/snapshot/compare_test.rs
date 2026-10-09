use super::*;

fn solid(width: u32, height: u32, pixel: [u8; 4]) -> Image {
    Image {
        width,
        height,
        rgba: pixel.repeat((width * height) as usize),
    }
}

fn with_pixel(mut image: Image, x: u32, y: u32, pixel: [u8; 4]) -> Image {
    let at = ((y * image.width + x) * 4) as usize;
    image.rgba[at..at + 4].copy_from_slice(&pixel);
    image
}

#[test]
fn rounding_within_the_channel_tolerance_is_no_difference() {
    let golden = solid(4, 4, [200, 100, 50, 255]);
    let actual = with_pixel(golden.clone(), 1, 1, [202, 99, 50, 255]);
    let diff = compare(&golden, &actual, Tolerance::default());
    assert_eq!((diff.differing, diff.max_delta), (0, 2));
    assert!(diff.within(Tolerance::default()));
}

#[test]
fn pixels_past_the_channel_tolerance_count_against_the_cap() {
    let golden = solid(4, 4, [200, 100, 50, 255]);
    let actual = with_pixel(
        with_pixel(golden.clone(), 0, 0, [0, 100, 50, 255]),
        3,
        3,
        [200, 100, 50, 128],
    );
    let diff = compare(&golden, &actual, Tolerance::default());
    assert_eq!((diff.differing, diff.max_delta), (2, 200));
    assert!(!diff.within(Tolerance::default()));
    assert!(
        diff.within(Tolerance::new(2, 2)),
        "a cap of two allows them"
    );
    assert_eq!(
        &diff.diff.data()[..4],
        &DIFFERING,
        "and the diff marks them"
    );
    let unchanged = &diff.diff.data()[4..8];
    assert_eq!(
        unchanged[0], unchanged[1],
        "a matching pixel is shown faded to grey"
    );
    assert!(unchanged[0] > 200);
}

#[test]
fn a_picture_of_another_size_differs_wherever_only_one_of_them_reaches() {
    let golden = solid(2, 2, [0, 0, 0, 255]);
    let actual = solid(3, 2, [0, 0, 0, 255]);
    let diff = compare(&golden, &actual, Tolerance::default());
    assert_eq!((diff.diff.width(), diff.diff.height()), (3, 2));
    assert_eq!((diff.differing, diff.max_delta), (2, 255));
}

#[test]
fn a_png_round_trips_to_the_same_straight_pixels() {
    let image = Image::of_pixmap(&{
        let mut pixmap = Pixmap::new(2, 1).unwrap();
        pixmap.fill(tiny_skia::Color::from_rgba8(10, 20, 30, 255));
        pixmap
    });
    let png = Pixmap::from_vec(image.rgba.clone(), IntSize::from_wh(2, 1).unwrap())
        .unwrap()
        .encode_png()
        .unwrap();
    assert_eq!(Image::decode(&png).unwrap(), image);
    assert!(Image::decode(b"not a png").is_err());
}

#[test]
fn a_line_diff_names_both_sides_and_shows_only_the_lines_that_moved() {
    let golden = "frame 100x40\nrect 0,0 100x40 fill=#ffffffff\ntext 8,8 40x16 \"Hi\"\n";
    let actual = "frame 100x40\nrect 0,0 100x40 fill=#ffffffff\ntext 12,8 40x16 \"Hi\"\n";
    let diff = line_diff(golden, actual, "golden", "actual");
    assert!(diff.starts_with("--- golden\n+++ actual\n"), "{diff}");
    assert!(
        diff.contains("\n-text 8,8 40x16 \"Hi\"\n+text 12,8 40x16 \"Hi\"\n"),
        "{diff}"
    );
    assert_eq!(line_diff(golden, golden, "golden", "actual"), "");
}
