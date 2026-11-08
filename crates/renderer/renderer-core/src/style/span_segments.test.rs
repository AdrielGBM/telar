use super::*;
use crate::style::Declared;

type Piece = (Range<usize>, Option<usize>);
type Ranges = &'static [(u32, u32)];

fn spans(ranges: &[(u32, u32)]) -> Vec<Span> {
    ranges
        .iter()
        .map(|&(start, end)| Span::new(start..end, Declared::default()))
        .collect()
}

fn cut(text: &str, ranges: &[(u32, u32)]) -> Vec<Piece> {
    let spans = spans(ranges);
    span_segments(text, &spans)
        .map(|segment| (segment.range, segment.span.map(|(index, _)| index)))
        .collect()
}

#[test]
fn text_is_cut_at_its_spans_and_the_gaps_are_kept() {
    let cases: &[(&str, &str, Ranges, &[Piece])] = &[
        ("no spans", "hello", &[], &[(0..5, None)]),
        ("empty text", "", &[(0, 3)], &[]),
        (
            "gaps on both sides",
            "hello world",
            &[(2, 4)],
            &[(0..2, None), (2..4, Some(0)), (4..11, None)],
        ),
        (
            "adjacent spans leave no gap",
            "abcdef",
            &[(0, 3), (3, 6)],
            &[(0..3, Some(0)), (3..6, Some(1))],
        ),
        (
            "overlap starts where the previous span ended",
            "abcdef",
            &[(0, 4), (2, 6)],
            &[(0..4, Some(0)), (4..6, Some(1))],
        ),
        (
            "a span inside the previous one is dropped",
            "abcdef",
            &[(0, 5), (1, 3)],
            &[(0..5, Some(0)), (5..6, None)],
        ),
        (
            "out of order starts where the previous span ended",
            "abcdef",
            &[(3, 5), (0, 6)],
            &[(0..3, None), (3..5, Some(0)), (5..6, Some(1))],
        ),
        (
            "past the end is clamped",
            "abc",
            &[(1, 99)],
            &[(0..1, None), (1..3, Some(0))],
        ),
        (
            "entirely past the end is dropped",
            "abc",
            &[(7, 9)],
            &[(0..3, None)],
        ),
        (
            "reversed range is dropped",
            "abc",
            &[(2, 1)],
            &[(0..3, None)],
        ),
        ("empty span is dropped", "abc", &[(1, 1)], &[(0..3, None)]),
        (
            "indices follow the slice, not the survivors",
            "abcdef",
            &[(2, 2), (2, 4)],
            &[(0..2, None), (2..4, Some(1)), (4..6, None)],
        ),
    ];
    for (name, text, ranges, expected) in cases {
        assert_eq!(&cut(text, ranges), expected, "{name}");
    }
}

#[test]
fn a_span_off_a_character_boundary_is_dropped_and_the_next_one_still_applies() {
    let text = "ñandú";
    let cases: &[(&str, Ranges, &[Piece])] = &[
        ("starts inside ñ", &[(1, 3)], &[(0..7, None)]),
        ("ends inside ú", &[(0, 6)], &[(0..7, None)]),
        (
            "next span still applies",
            &[(1, 3), (2, 5)],
            &[(0..2, None), (2..5, Some(1)), (5..7, None)],
        ),
    ];
    for (name, ranges, expected) in cases {
        assert_eq!(&cut(text, ranges), expected, "{name}");
    }
}

#[test]
fn segments_always_cover_the_whole_text_in_order() {
    let text = "ñandú y más";
    let spans = spans(&[(0, 2), (1, 4), (3, 3), (6, 40), (2, 8)]);
    let mut at = 0;
    for segment in span_segments(text, &spans) {
        assert_eq!(segment.range.start, at);
        assert!(!segment.range.is_empty());
        at = segment.range.end;
    }
    assert_eq!(at, text.len());
}
