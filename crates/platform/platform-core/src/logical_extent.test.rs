use super::*;

#[test]
fn rounds_a_fractional_result_to_the_nearest_pixel() {
    assert_eq!(logical_extent(1366, 1.25), 1093);
}

#[test]
fn converts_across_common_scale_factors() {
    let table = [
        (1920, 1.0, 1920),
        (1920, 1.25, 1536),
        (1920, 1.5, 1280),
        (1920, 2.0, 960),
        (1080, 1.25, 864),
        (1080, 1.75, 617),
        (2560, 2.5, 1024),
        (1366, 3.0, 455),
        (0, 2.0, 0),
    ];
    for (physical, scale, expected) in table {
        assert_eq!(
            logical_extent(physical, scale),
            expected,
            "{physical} at {scale}"
        );
    }
}
