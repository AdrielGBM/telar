use super::*;

#[test]
fn rgb_sets_alpha_to_one() {
    let color = Color::rgb(0.5, 0.5, 0.5);
    assert_eq!(color.a, 1.0);
}

#[test]
fn rgba_stores_all_components() {
    let color = Color::rgba(0.1, 0.2, 0.3, 0.4);
    assert_eq!(color.r, 0.1);
    assert_eq!(color.g, 0.2);
    assert_eq!(color.b, 0.3);
    assert_eq!(color.a, 0.4);
}

#[test]
fn from_rgb_u8_normalizes_to_float() {
    let color = Color::from_rgb_u8(255, 0, 0);
    assert_eq!(color.r, 1.0);
    assert_eq!(color.g, 0.0);
    assert_eq!(color.b, 0.0);
    assert_eq!(color.a, 1.0);
}

#[test]
fn to_rgba8_white() {
    assert_eq!(Color::WHITE.to_rgba8(), [255, 255, 255, 255]);
}

#[test]
fn to_rgba8_black() {
    assert_eq!(Color::BLACK.to_rgba8(), [0, 0, 0, 255]);
}

#[test]
fn to_rgba8_transparent() {
    assert_eq!(Color::TRANSPARENT.to_rgba8(), [0, 0, 0, 0]);
}

#[test]
fn to_rgba8_clamps_above_one() {
    let color = Color::rgba(2.0, 2.0, 2.0, 2.0);
    assert_eq!(color.to_rgba8(), [255, 255, 255, 255]);
}

#[test]
fn to_rgba8_clamps_below_zero() {
    let color = Color::rgba(-1.0, -1.0, -1.0, -1.0);
    assert_eq!(color.to_rgba8(), [0, 0, 0, 0]);
}

/// Truncating threw away a whole channel step, and only the backends that write a colour as text went through here — so a document drew a theme one step darker than the same theme rasterised.
#[test]
fn to_rgba8_rounds_to_the_nearest_channel() {
    let color = Color::rgba(0.5, 0.47, 0.98, 0.1);
    assert_eq!(color.to_rgba8(), [128, 120, 250, 26]);
}

/// A colour written as hex is the bytes it was written with, whichever way it has been round-tripped.
#[test]
fn to_rgba8_returns_the_bytes_a_hex_colour_was_written_with() {
    for hex in ["#0e1017", "#3b5bdb", "#5b7cfa", "#f2b53c", "#e2e5ee"] {
        let color = Color::from_hex(hex).unwrap();
        let [r, g, b, _] = color.to_rgba8();
        assert_eq!(format!("#{r:02x}{g:02x}{b:02x}"), hex);
    }
}

#[test]
fn constant_red_components() {
    assert_eq!(Color::RED.r, 1.0);
    assert_eq!(Color::RED.g, 0.0);
    assert_eq!(Color::RED.b, 0.0);
}

#[test]
fn constant_green_components() {
    assert_eq!(Color::GREEN.r, 0.0);
    assert_eq!(Color::GREEN.g, 1.0);
    assert_eq!(Color::GREEN.b, 0.0);
}

#[test]
fn constant_blue_components() {
    assert_eq!(Color::BLUE.r, 0.0);
    assert_eq!(Color::BLUE.g, 0.0);
    assert_eq!(Color::BLUE.b, 1.0);
}

fn assert_rgb(color: Color, r: f32, g: f32, b: f32, tol: f32) {
    assert!((color.r - r).abs() < tol, "r: {} != {}", color.r, r);
    assert!((color.g - g).abs() < tol, "g: {} != {}", color.g, g);
    assert!((color.b - b).abs() < tol, "b: {} != {}", color.b, b);
}

#[test]
fn from_hex_six_digits() {
    assert_rgb(Color::from_hex("#ff0000").unwrap(), 1.0, 0.0, 0.0, 1e-5);
}

#[test]
fn from_hex_without_prefix() {
    assert_rgb(Color::from_hex("00ff00").unwrap(), 0.0, 1.0, 0.0, 1e-5);
}

#[test]
fn from_hex_three_digits_expands() {
    let short = Color::from_hex("#f00").unwrap();
    assert_rgb(short, 1.0, 0.0, 0.0, 1e-5);
}

#[test]
fn from_hex_eight_digits_reads_alpha() {
    let color = Color::from_hex("#0000ff80").unwrap();
    assert_rgb(color, 0.0, 0.0, 1.0, 1e-5);
    assert!(
        (color.a - 128.0 / 255.0).abs() < 1e-5,
        "the last byte is alpha: {}",
        color.a
    );
}

#[test]
fn from_hex_four_digits_reads_alpha() {
    let color = Color::from_hex("#00f8").unwrap();
    assert_rgb(color, 0.0, 0.0, 1.0, 1e-5);
    assert!(
        (color.a - 0x88 as f32 / 255.0).abs() < 1e-5,
        "the fourth nibble is alpha: {}",
        color.a
    );
}

#[test]
fn from_hex_rejects_invalid() {
    assert!(
        Color::from_hex("#gggggg").is_none(),
        "non-hex digits are not a colour"
    );
    assert!(
        Color::from_hex("12345").is_none(),
        "five digits match no hex form"
    );
    assert!(
        Color::from_hex("").is_none(),
        "an empty string is not a colour"
    );
}

#[test]
fn from_oklch_white() {
    assert_rgb(Color::from_oklch(1.0, 0.0, 0.0), 1.0, 1.0, 1.0, 1e-4);
}

#[test]
fn from_oklch_black() {
    assert_rgb(Color::from_oklch(0.0, 0.0, 0.0), 0.0, 0.0, 0.0, 1e-4);
}

#[test]
fn from_oklch_srgb_red() {
    assert_rgb(
        Color::from_oklch(0.627_955, 0.257_683, 29.233_88),
        1.0,
        0.0,
        0.0,
        2e-2,
    );
}

#[test]
fn from_oklcha_sets_alpha() {
    assert_eq!(Color::from_oklcha(0.5, 0.1, 120.0, 0.25).a, 0.25);
}

#[test]
fn to_oklcha_round_trips_srgb() {
    for &color in &[
        Color::rgb(0.8, 0.2, 0.3),
        Color::rgb(0.1, 0.6, 0.9),
        Color::rgb(0.5, 0.5, 0.5),
        Color::rgba(0.2, 0.7, 0.4, 0.6),
    ] {
        let (l, c, h, a) = color.to_oklcha();
        let back = Color::from_oklcha(l, c, h, a);
        assert!(
            (back.r - color.r).abs() < 1e-4,
            "r: {} != {}",
            back.r,
            color.r
        );
        assert!(
            (back.g - color.g).abs() < 1e-4,
            "g: {} != {}",
            back.g,
            color.g
        );
        assert!(
            (back.b - color.b).abs() < 1e-4,
            "b: {} != {}",
            back.b,
            color.b
        );
        assert!(
            (back.a - color.a).abs() < 1e-6,
            "a: {} != {}",
            back.a,
            color.a
        );
    }
}

#[test]
fn to_oklcha_gray_is_achromatic() {
    let (_, c, _, _) = Color::rgb(0.5, 0.5, 0.5).to_oklcha();
    assert!(c < 1e-4, "expected near-zero chroma, got {c}");
}

#[test]
fn relative_luminance_black_and_white() {
    assert!(
        Color::BLACK.relative_luminance().abs() < 1e-6,
        "black is the bottom of the scale: {}",
        Color::BLACK.relative_luminance()
    );
    assert!(
        (Color::WHITE.relative_luminance() - 1.0).abs() < 1e-6,
        "white is the top of it: {}",
        Color::WHITE.relative_luminance()
    );
}

#[test]
fn contrast_ratio_extremes_and_identity() {
    assert!(
        (Color::WHITE.contrast_ratio(Color::BLACK) - 21.0).abs() < 1e-3,
        "21:1 is the widest ratio there is: {}",
        Color::WHITE.contrast_ratio(Color::BLACK)
    );
    assert!(
        (Color::BLACK.contrast_ratio(Color::WHITE) - 21.0).abs() < 1e-3,
        "and the ratio does not depend on the order: {}",
        Color::BLACK.contrast_ratio(Color::WHITE)
    );
    assert!(
        (Color::RED.contrast_ratio(Color::RED) - 1.0).abs() < 1e-6,
        "a colour against itself is 1:1: {}",
        Color::RED.contrast_ratio(Color::RED)
    );
}

#[test]
fn most_readable_picks_the_higher_contrast_foreground() {
    let ink = Color::from_rgb_u8(20, 20, 25);
    let paper = Color::from_rgb_u8(240, 240, 245);
    let light_bg = Color::from_rgb_u8(230, 220, 180);
    let dark_bg = Color::from_rgb_u8(40, 50, 70);
    assert_eq!(light_bg.most_readable(&[ink, paper]), ink);
    assert_eq!(dark_bg.most_readable(&[ink, paper]), paper);
    assert_eq!(light_bg.most_readable(&[]), light_bg);
}

#[test]
fn oklch_round_trip_is_stable() {
    let start = Color::rgb(0.1, 0.6, 0.9);
    let (l1, c1, h1, _) = start.to_oklcha();
    let (l2, c2, h2, _) = Color::from_oklcha(l1, c1, h1, 1.0).to_oklcha();
    assert!((l1 - l2).abs() < 1e-3, "l: {l1} != {l2}");
    assert!((c1 - c2).abs() < 1e-3, "c: {c1} != {c2}");
    assert!((h1 - h2).abs() < 1e-2, "h: {h1} != {h2}");
}
