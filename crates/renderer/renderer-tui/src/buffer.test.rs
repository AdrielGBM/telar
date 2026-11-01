use super::*;

fn buffer(cols: u16, rows: u16) -> CellBuffer {
    CellBuffer::new(cols, rows, Rgb::BLACK)
}

#[test]
fn an_unchanged_frame_writes_nothing() {
    let a = buffer(10, 3);
    let b = buffer(10, 3);
    let mut out = Vec::new();
    b.diff_into(&a, ColorDepth::TrueColor, &mut out);
    assert!(out.is_empty(), "got {:?}", String::from_utf8_lossy(&out));
}

#[test]
fn only_the_changed_cell_is_written() {
    let previous = buffer(10, 3);
    let mut next = buffer(10, 3);
    next.put(4, 1, Grapheme::from('X'), Rgb::WHITE, Attrs::NONE);
    let mut out = Vec::new();
    next.diff_into(&previous, ColorDepth::TrueColor, &mut out);
    let s = String::from_utf8(out).unwrap();
    assert!(s.contains("\x1b[2;5H"), "cursor move missing: {s:?}");
    assert_eq!(s.matches('X').count(), 1);
}

#[test]
fn a_resize_repaints_everything() {
    let previous = buffer(4, 1);
    let next = buffer(6, 1);
    let mut out = Vec::new();
    next.diff_into(&previous, ColorDepth::TrueColor, &mut out);
    assert!(
        !out.is_empty(),
        "a resize invalidates the whole buffer, so the flush cannot be empty"
    );
}

#[test]
fn a_wide_glyph_claims_two_columns() {
    let mut b = buffer(10, 1);
    let taken = b.put(0, 0, Grapheme::new("漢"), Rgb::WHITE, Attrs::NONE);
    assert_eq!(taken, 2);
    assert!(
        b.get(1, 0).unwrap().attrs.contains(Attrs::WIDE_TAIL),
        "the cell after a wide glyph is its tail: {:?}",
        b.get(1, 0)
    );
}

#[test]
fn a_wide_glyph_that_would_overflow_is_dropped() {
    let mut b = buffer(2, 1);
    b.put(1, 0, Grapheme::new("漢"), Rgb::WHITE, Attrs::NONE);
    assert_eq!(b.get(1, 0).unwrap().glyph.as_str(), " ");
}

#[test]
fn turning_an_attribute_off_resets_the_pen() {
    let previous = buffer(4, 1);
    let mut next = buffer(4, 1);
    next.put(0, 0, Grapheme::from('a'), Rgb::WHITE, Attrs::BOLD);
    next.put(1, 0, Grapheme::from('b'), Rgb::WHITE, Attrs::NONE);
    let mut out = Vec::new();
    next.diff_into(&previous, ColorDepth::TrueColor, &mut out);
    let s = String::from_utf8(out).unwrap();
    let bold = s.find("\x1b[1m").expect("bold set");
    let reset = s[bold..]
        .find("\x1b[0m")
        .expect("pen reset before the plain cell");
    assert!(
        s[bold + reset..].contains('b'),
        "the reset must be followed by the plain text: {s}"
    );
}

#[test]
fn a_hyperlink_uri_cannot_end_its_sequence_early_with_a_c1_control() {
    let mut buf = buffer(2, 1);
    let link = buf.link_id("https://a\u{9b}31m\u{9c}b\x07c");
    buf.get_mut(0, 0).unwrap().link = link;
    buf.put(0, 0, Grapheme::from('x'), Rgb::WHITE, Attrs::NONE);
    buf.get_mut(0, 0).unwrap().link = link;
    let mut out = Vec::new();
    buf.diff_into(&buffer(0, 0), ColorDepth::TrueColor, &mut out);
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("\x1b]8;;https://a31mbc\x1b\\"), "{text:?}");
    assert!(
        !text
            .chars()
            .any(|c| ('\u{80}'..='\u{9f}').contains(&c) || c == '\x07')
    );
}
