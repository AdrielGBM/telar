use super::*;

#[test]
fn opaque_source_replaces() {
    let dst = Rgb::BLACK;
    assert_eq!(dst.under(Color::WHITE), Rgb::WHITE);
}

#[test]
fn half_alpha_meets_in_the_middle() {
    let out = Rgb::BLACK.under(Color::rgba(1.0, 1.0, 1.0, 0.5));
    assert!(out.r.abs_diff(128) <= 1, "got {}", out.r);
}

#[test]
fn transparent_source_leaves_destination() {
    let dst = Rgb {
        r: 10,
        g: 20,
        b: 30,
    };
    assert_eq!(dst.under(Color::rgba(1.0, 0.0, 0.0, 0.0)), dst);
}

#[test]
fn greys_land_on_the_grey_ramp() {
    assert_eq!(
        to_ansi256(Rgb {
            r: 128,
            g: 128,
            b: 128
        }),
        244
    );
}

#[test]
fn saturated_colors_land_in_the_cube() {
    assert_eq!(to_ansi256(Rgb { r: 255, g: 0, b: 0 }), 196);
}

#[test]
fn pure_colors_reach_their_ansi16_slot() {
    assert_eq!(to_ansi16(Rgb::BLACK), 0);
    assert_eq!(to_ansi16(Rgb::WHITE), 15);
    assert_eq!(to_ansi16(Rgb { r: 255, g: 0, b: 0 }), 9);
}
