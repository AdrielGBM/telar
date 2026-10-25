use std::borrow::Cow;
use std::fmt::Debug;
use std::sync::Arc;
use std::time::Duration;

use super::*;

fn round_trip<T: Wire + PartialEq + Debug>(value: T) {
    assert_eq!(decode::<T>(&encode(&value)), Ok(value));
}

#[test]
fn scalars_round_trip_at_their_edges() {
    round_trip(true);
    round_trip(false);
    round_trip(u8::MAX);
    round_trip(u16::MAX);
    round_trip(u32::MAX);
    round_trip(u64::MAX);
    round_trip(i32::MIN);
    round_trip(usize::MAX);
    round_trip(-0.5f32);
    round_trip(f64::MIN_POSITIVE);
    round_trip('🙂');
}

#[test]
fn text_round_trips_in_every_owner() {
    round_trip(String::from("añ🙂"));
    round_trip(Box::<str>::from("box"));
    round_trip(Arc::<str>::from("arc"));
    round_trip(Cow::<'static, str>::Borrowed("borrowed"));
}

#[test]
fn launch_arguments_round_trip() {
    round_trip(vec![
        String::from("--open"),
        String::new(),
        String::from("ruta con espacios"),
    ]);
    round_trip(Vec::<String>::new());
}

#[test]
fn a_blob_round_trips_whether_there_or_not() {
    round_trip(Some(vec![0u8, 1, 0xff, 0x80]));
    round_trip(None::<Vec<u8>>);
    round_trip(String::from(r#"{"count":3}"#));
}

#[test]
fn durations_round_trip_with_their_nanoseconds() {
    round_trip(Duration::ZERO);
    round_trip(Duration::new(3, 999_999_999));
    round_trip(Duration::MAX);
    round_trip(Some(Duration::from_millis(16)));
    round_trip(None::<Duration>);
}

#[test]
fn a_duration_with_a_whole_second_of_nanoseconds_is_refused() {
    let mut w = Writer::new();
    w.varint(0);
    w.varint(1_000_000_000);
    assert_eq!(
        decode::<Duration>(w.as_slice()),
        Err(WireError::Invalid(
            "a duration with a second of nanoseconds or more"
        ))
    );
}

#[test]
fn containers_round_trip() {
    round_trip(Box::new(7u64));
    round_trip(Arc::new(String::from("shared")));
    round_trip(Arc::<[u32]>::from(vec![1, 2, 300]));
    round_trip([1.0f32, 2.0, 3.0]);
    round_trip((5u32, String::from("pair")));
    round_trip(2usize..9);
}

#[test]
fn a_surrogate_is_no_char() {
    let mut w = Writer::new();
    w.varint(0xd800);
    assert_eq!(
        decode::<char>(w.as_slice()),
        Err(WireError::Invalid("a char that is no Unicode scalar"))
    );
}

#[test]
fn bytes_left_after_the_value_are_refused() {
    let mut bytes = encode(&7u64);
    bytes.push(0);
    assert_eq!(decode::<u64>(&bytes), Err(WireError::Trailing(1)));
}

#[test]
fn an_option_tag_other_than_none_or_some_is_refused() {
    assert_eq!(
        decode::<Option<u8>>(&[2]),
        Err(WireError::Tag {
            of: "Option",
            tag: 2
        })
    );
}

#[derive(Debug, PartialEq)]
struct Pair {
    left: u32,
    right: Option<String>,
}

wire_struct!(Pair { left, right });

#[derive(Debug, PartialEq)]
enum Shape {
    Empty,
    Tuple(u8, String),
    Named { width: f32, height: f32 },
    Nested(Box<Shape>),
}

wire_enum!(Shape {
    Empty = 0,
    Tuple(a, b) = 1,
    Named { width, height } = 5,
    Nested(inner) = 9,
});

#[test]
fn macro_spelled_values_round_trip_in_every_variant_shape() {
    round_trip(Pair {
        left: 3,
        right: Some(String::from("r")),
    });
    round_trip(Shape::Empty);
    round_trip(Shape::Tuple(2, String::from("t")));
    round_trip(Shape::Named {
        width: 1.0,
        height: 2.0,
    });
    round_trip(Shape::Nested(Box::new(Shape::Named {
        width: 3.0,
        height: 4.0,
    })));
}

#[test]
fn an_enum_is_spelled_by_its_written_tag_not_its_declaration_order() {
    assert_eq!(
        encode(&Shape::Named {
            width: 0.0,
            height: 0.0
        })[0],
        5
    );
    assert_eq!(
        decode::<Shape>(&[3]),
        Err(WireError::Tag {
            of: "Shape",
            tag: 3
        })
    );
}

#[test]
fn the_bytes_of_every_primitive_are_pinned_to_the_wire_version() {
    let mut w = Writer::new();
    true.put(&mut w);
    false.put(&mut w);
    u8::MAX.put(&mut w);
    u16::MAX.put(&mut w);
    u32::MAX.put(&mut w);
    u64::MAX.put(&mut w);
    i32::MIN.put(&mut w);
    usize::MAX.put(&mut w);
    (-0.5f32).put(&mut w);
    f64::MIN_POSITIVE.put(&mut w);
    '🙂'.put(&mut w);
    String::from("añ🙂").put(&mut w);
    Some(vec![0u8, 1, 0xff, 0x80]).put(&mut w);
    None::<Vec<u8>>.put(&mut w);
    Duration::new(3, 999_999_999).put(&mut w);
    Duration::MAX.put(&mut w);
    Arc::<[u32]>::from(vec![1, 2, 300]).put(&mut w);
    [1.0f32, 2.0, 3.0].put(&mut w);
    (5u32, String::from("pair")).put(&mut w);
    (2usize..9).put(&mut w);
    Pair {
        left: 300,
        right: Some("x".into()),
    }
    .put(&mut w);
    for shape in [
        Shape::Empty,
        Shape::Tuple(7, "t".into()),
        Shape::Named {
            width: 1.5,
            height: 2.5,
        },
        Shape::Nested(Box::new(Shape::Empty)),
    ] {
        shape.put(&mut w);
    }
    crate::assert_golden!("src/wire_golden/primitives.hex", w.as_slice());
}
