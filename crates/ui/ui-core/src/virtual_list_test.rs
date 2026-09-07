use super::*;

#[test]
fn the_window_covers_the_screen_plus_its_overscan() {
    let (first, last) = visible_window(0.0, 100.0, 20.0, 1000, 0);
    assert_eq!((first, last), (0, 6));

    let (first, last) = visible_window(200.0, 100.0, 20.0, 1000, 2);
    assert_eq!((first, last), (8, 18));

    let (first, _) = visible_window(199.0, 100.0, 20.0, 1000, 0);
    assert_eq!(first, 9, "row 9 is still showing its last pixel");
}

#[test]
fn the_window_is_clamped_at_both_ends() {
    assert_eq!(visible_window(0.0, 100.0, 20.0, 1000, 5), (0, 11));
    assert_eq!(visible_window(19_800.0, 100.0, 20.0, 1000, 5), (985, 1000));
    let (first, last) = visible_window(100_000.0, 100.0, 20.0, 10, 0);
    assert!(first <= last && last <= 10, "got {first}..{last}");
}

#[test]
fn an_unmeasured_viewport_renders_everything_rather_than_nothing() {
    assert_eq!(visible_window(0.0, 0.0, 20.0, 40, 0), (0, 40));
    assert_eq!(visible_window(0.0, 100.0, 0.0, 40, 0), (0, 40));
    assert_eq!(
        visible_window(0.0, 100.0, 20.0, 0, 0),
        (0, 0),
        "an empty list is empty"
    );
}
