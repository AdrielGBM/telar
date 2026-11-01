#[allow(unused_imports)]
use crate::*;

/// The scope a `.rsx` body sees: `telar`'s glob brings the markers beside the surface's `kept`, with nothing of the same name to make them ambiguous.
#[test]
fn the_markers_are_reached_through_telars_glob() {
    let later_one = later(|| 2);
    let spot = now(|| 3);
    assert_eq!(later_one() + spot(), 5);
}
