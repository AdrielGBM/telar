use std::borrow::Cow;

use geometry_core::{Insets, Rect, Size};
use wire_core::{decode, encode};

use crate::SegmentNodeInfo;

#[test]
fn a_walked_tree_round_trips_and_owns_its_names() {
    let nodes = vec![
        SegmentNodeInfo {
            id: 0,
            name: Cow::Borrowed("Root"),
            depth: 0,
            rect: Rect::new(0.0, 0.0, 800.0, 600.0),
            padding: Insets::new(1.0, 2.0, 3.0, 4.0),
            margin: Insets::default(),
            border: Insets::new(1.0, 1.0, 1.0, 1.0),
            gap: Size::new(8.0, 4.0),
        },
        SegmentNodeInfo {
            id: 1,
            name: Cow::Owned("Fila".to_owned()),
            depth: 1,
            rect: Rect::new(10.0, 10.0, 100.0, 20.0),
            padding: Insets::default(),
            margin: Insets::new(0.0, 0.0, 4.0, 0.0),
            border: Insets::default(),
            gap: Size::ZERO,
        },
    ];
    let read = decode::<Vec<SegmentNodeInfo>>(&encode(&nodes)).unwrap();
    assert_eq!(format!("{read:?}"), format!("{nodes:?}"));
    assert!(read.iter().all(|node| matches!(node.name, Cow::Owned(_))));
}

#[test]
fn the_bytes_of_a_walked_tree_are_pinned_to_the_wire_version() {
    let nodes = vec![
        SegmentNodeInfo {
            id: 0,
            name: Cow::Borrowed("Root"),
            depth: 0,
            rect: Rect::new(0.0, 0.0, 800.0, 600.0),
            padding: Insets::new(1.0, 2.0, 3.0, 4.0),
            margin: Insets::default(),
            border: Insets::new(1.0, 1.0, 1.0, 1.0),
            gap: Size::new(8.0, 4.0),
        },
        SegmentNodeInfo {
            id: 1,
            name: Cow::Owned("Fila".to_owned()),
            depth: 1,
            rect: Rect::new(10.0, 10.0, 100.0, 20.0),
            padding: Insets::default(),
            margin: Insets::new(0.0, 0.0, 4.0, 0.0),
            border: Insets::default(),
            gap: Size::ZERO,
        },
    ];
    wire_core::assert_golden!("src/wire_golden/segment.hex", &encode(&nodes));
}
