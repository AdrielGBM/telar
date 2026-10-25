use super::*;

#[test]
fn varints_round_trip_at_every_width() {
    let values = [
        0,
        1,
        0x7f,
        0x80,
        0x3fff,
        0x4000,
        u64::from(u32::MAX),
        u64::MAX,
    ];
    let mut w = Writer::default();
    for value in values {
        w.varint(value);
    }
    let mut r = Reader::new(w.as_slice());
    for value in values {
        assert_eq!(r.varint(), Ok(value));
    }
    assert!(r.is_empty());
}

#[test]
fn a_varint_past_64_bits_is_refused() {
    let mut r = Reader::new(&[0xff; 11]);
    assert_eq!(
        r.varint(),
        Err(WireError::Invalid("varint longer than 64 bits"))
    );
}

#[test]
fn a_count_beyond_the_bytes_left_is_refused_before_anything_is_reserved() {
    let mut w = Writer::default();
    w.len(1 << 40);
    w.u8(0);
    assert_eq!(Reader::new(w.as_slice()).len(), Err(WireError::Truncated));
}

#[test]
fn text_round_trips_and_invalid_utf8_is_refused() {
    let mut w = Writer::default();
    w.str("añ🙂");
    assert_eq!(Reader::new(w.as_slice()).str(), Ok("añ🙂"));
    assert_eq!(
        Reader::new(&[2, 0xc3, 0x28]).str(),
        Err(WireError::Invalid("text is not UTF-8"))
    );
}

#[test]
fn scalars_are_little_endian() {
    let mut w = Writer::default();
    w.u16(0x0102);
    w.u64(0x0102_0304_0506_0708);
    w.f32(1.5);
    assert_eq!(
        w.as_slice(),
        [
            [0x02, 0x01].as_slice(),
            &[8, 7, 6, 5, 4, 3, 2, 1],
            &1.5f32.to_le_bytes()
        ]
        .concat()
    );
    let mut r = Reader::new(w.as_slice());
    assert_eq!(r.u16(), Ok(0x0102));
    assert_eq!(r.u64(), Ok(0x0102_0304_0506_0708));
    assert_eq!(r.f32(), Ok(1.5));
    assert_eq!(r.u8(), Err(WireError::Truncated));
}
