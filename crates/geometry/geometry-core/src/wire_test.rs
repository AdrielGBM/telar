use std::fmt::Debug;

use wire_core::{Wire, decode, encode};

use crate::{BorderRadius, Color, Insets, Point, Rect, Size};

fn round_trip<T: Wire + PartialEq + Debug>(value: T) {
    assert_eq!(decode::<T>(&encode(&value)), Ok(value));
}

#[test]
fn every_geometry_value_round_trips() {
    round_trip(Point::new(-1.5, 2.25));
    round_trip(Size::new(320.0, 0.5));
    round_trip(Rect::new(1.0, -2.0, 30.0, 40.0));
    round_trip(Color::rgba(0.1, 0.2, 0.3, 0.4));
    round_trip(BorderRadius {
        top_left: 1.0,
        top_right: 2.0,
        bottom_right: 3.0,
        bottom_left: 4.0,
    });
    round_trip(Insets::new(1.0, 2.0, 3.0, 4.0));
}

#[test]
fn the_bytes_of_every_geometry_value_are_pinned_to_the_wire_version() {
    let mut bytes = encode(&Point::new(-1.5, 2.25));
    bytes.extend(encode(&Size::new(320.0, 0.5)));
    bytes.extend(encode(&Rect::new(1.0, -2.0, 30.0, 40.0)));
    bytes.extend(encode(&Color::rgba(0.1, 0.2, 0.3, 0.4)));
    bytes.extend(encode(&BorderRadius {
        top_left: 1.0,
        top_right: 2.0,
        bottom_right: 3.0,
        bottom_left: 4.0,
    }));
    bytes.extend(encode(&Insets::new(1.0, 2.0, 3.0, 4.0)));
    wire_core::assert_golden!("src/wire_golden/geometry.hex", &bytes);
}
