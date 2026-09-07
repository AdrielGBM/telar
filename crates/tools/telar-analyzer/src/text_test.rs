use super::*;

// The column arriving from the editor counts UTF-16 units; slicing the line by it directly panicked the moment anything before the cursor was multi-byte.
#[test]
fn word_at_cursor_takes_a_utf16_column_not_a_byte_offset() {
    let line = r#"    text "héllo world""#;
    // The first `l`: its UTF-16 column lands inside the two bytes of `é`, so slicing the line by that column is a non-char-boundary panic, not merely a wrong answer.
    let byte = line.find("llo").unwrap();
    let utf16 = byte_to_utf16(line, byte);
    assert!(
        !line.is_char_boundary(utf16 as usize),
        "the fixture needs a multibyte char here, or the test proves nothing: {line}"
    );
    assert_eq!(word_at_cursor(line, utf16).1, "héllo");
    assert_eq!(word_at_cursor(line, 9_999).1, "");
}

#[test]
fn ident_at_finds_the_word_under_the_cursor() {
    let line = "let total_count = 1;";
    let (start, word) = ident_at(line, 7).unwrap();
    assert_eq!((start, word), (4, "total_count"));
    assert!(
        ident_at(line, 16).is_none(),
        "there is no word under a cursor past the end of the line"
    );
}

#[test]
fn leading_token_skips_indentation() {
    assert_eq!(leading_token("    btn \"+\""), Some((4, "btn")));
    assert_eq!(leading_token("   "), None);
}

#[test]
fn offset_and_byte_offset_round_trip_multibyte() {
    let src = "[view]\ncol é x\n";
    let byte = byte_offset(src, 1, 6).unwrap();
    let pos = offset_to_position(src, byte);
    assert_eq!((pos.line, pos.character), (1, 6));
}
