use wire_core::{WireError, decode, encode};

use super::*;
use crate::Role;

#[test]
fn a_heading_level_counts_one_to_six_and_nothing_else() {
    assert_eq!(HeadingLevel::new(0), None);
    assert_eq!(HeadingLevel::new(7), None);
    let counted: Vec<u8> = (1..=6)
        .filter_map(HeadingLevel::new)
        .map(HeadingLevel::get)
        .collect();
    assert_eq!(counted, [1, 2, 3, 4, 5, 6]);
    assert_eq!(HeadingLevel::ALL.map(HeadingLevel::get), [1, 2, 3, 4, 5, 6]);
}

#[test]
fn a_level_outside_the_outline_is_read_as_the_nearest_one() {
    assert_eq!(HeadingLevel::nearest(0), HeadingLevel::One);
    assert_eq!(HeadingLevel::nearest(4), HeadingLevel::Four);
    assert_eq!(HeadingLevel::nearest(u8::MAX), HeadingLevel::Six);
}

#[test]
fn a_heading_of_level_zero_arriving_as_bytes_is_refused() {
    let mut bytes = encode(&Role::Heading(HeadingLevel::One));
    *bytes.last_mut().expect("a heading spells its level") = 0;
    assert_eq!(
        decode::<Role>(&bytes),
        Err(WireError::Tag {
            of: "HeadingLevel",
            tag: 0
        })
    );
}

#[test]
fn a_set_position_of_zero_arriving_as_bytes_is_refused() {
    let place = SetPosition {
        level: NonZeroU32::MIN,
        position: NonZeroU32::MIN,
        size: NonZeroU32::MIN,
    };
    let mut bytes = encode(&place);
    *bytes.last_mut().expect("a place spells its size") = 0;
    assert_eq!(
        decode::<SetPosition>(&bytes),
        Err(WireError::Invalid("a zero where none may be"))
    );
}
