use super::*;

/// What the buffer shows in each column of a row, for comparison with the model. A wide glyph's tail column is empty, exactly as it is on the terminal.
fn buffer_row(buf: &CellBuffer, row: u16) -> Vec<String> {
    (0..buf.cols())
        .map(|col| {
            let cell = buf.get(col, row).expect("in range");
            if cell.attrs.contains(Attrs::WIDE_TAIL) {
                String::new()
            } else {
                cell.glyph.as_str().to_string()
            }
        })
        .collect()
}

fn assert_screen_matches(model: &TerminalModel, buf: &CellBuffer) {
    for row in 0..buf.rows() {
        assert_eq!(
            model.row(row),
            buffer_row(buf, row),
            "row {row} on the terminal does not match the frame that was drawn"
        );
    }
}

fn write(buf: &mut CellBuffer, col: u16, row: u16, text: &str) {
    let mut at = col;
    for g in text.chars() {
        at += buf.put(at, row, Grapheme::from(g), Rgb::WHITE, Attrs::NONE);
    }
}

#[test]
fn a_second_frame_leaves_the_terminal_showing_it() {
    let mut model = TerminalModel::new(20, 3);
    let empty = CellBuffer::new(0, 0, Rgb::BLACK);

    let mut first = CellBuffer::new(20, 3, Rgb::BLACK);
    write(&mut first, 0, 0, "overview section");
    write(&mut first, 2, 1, "a long line here");
    let mut out = Vec::new();
    first.diff_into(&empty, ColorDepth::TrueColor, &mut out);
    model.apply(&out);
    assert_screen_matches(&model, &first);

    let mut second = CellBuffer::new(20, 3, Rgb::BLACK);
    write(&mut second, 0, 0, "typography");
    write(&mut second, 2, 1, "dog");
    out.clear();
    second.diff_into(&first, ColorDepth::TrueColor, &mut out);
    model.apply(&out);
    assert_screen_matches(&model, &second);
}

#[test]
fn a_wide_glyph_replaced_by_a_narrow_one_leaves_no_tail() {
    let mut model = TerminalModel::new(8, 1);
    let empty = CellBuffer::new(0, 0, Rgb::BLACK);

    let mut first = CellBuffer::new(8, 1, Rgb::BLACK);
    first.put(0, 0, Grapheme::new("漢"), Rgb::WHITE, Attrs::NONE);
    first.put(2, 0, Grapheme::from('x'), Rgb::WHITE, Attrs::NONE);
    let mut out = Vec::new();
    first.diff_into(&empty, ColorDepth::TrueColor, &mut out);
    model.apply(&out);
    assert_screen_matches(&model, &first);

    let mut second = CellBuffer::new(8, 1, Rgb::BLACK);
    write(&mut second, 0, 0, "ab");
    second.put(2, 0, Grapheme::from('x'), Rgb::WHITE, Attrs::NONE);
    out.clear();
    second.diff_into(&first, ColorDepth::TrueColor, &mut out);
    model.apply(&out);
    assert_screen_matches(&model, &second);
}

#[test]
fn a_narrow_glyph_replaced_by_a_wide_one_covers_both_columns() {
    let mut model = TerminalModel::new(8, 1);
    let empty = CellBuffer::new(0, 0, Rgb::BLACK);

    let mut first = CellBuffer::new(8, 1, Rgb::BLACK);
    write(&mut first, 0, 0, "ab");
    let mut out = Vec::new();
    first.diff_into(&empty, ColorDepth::TrueColor, &mut out);
    model.apply(&out);

    let mut second = CellBuffer::new(8, 1, Rgb::BLACK);
    second.put(0, 0, Grapheme::new("漢"), Rgb::WHITE, Attrs::NONE);
    out.clear();
    second.diff_into(&first, ColorDepth::TrueColor, &mut out);
    model.apply(&out);
    assert_screen_matches(&model, &second);
}

#[test]
fn a_narrow_glyph_over_a_wide_head_leaves_no_orphaned_tail() {
    let mut model = TerminalModel::new(8, 1);
    let empty = CellBuffer::new(0, 0, Rgb::BLACK);

    let mut first = CellBuffer::new(8, 1, Rgb::BLACK);
    first.put(0, 0, Grapheme::new("漢"), Rgb::WHITE, Attrs::NONE);
    first.put(2, 0, Grapheme::from('x'), Rgb::WHITE, Attrs::NONE);
    let mut out = Vec::new();
    first.diff_into(&empty, ColorDepth::TrueColor, &mut out);
    model.apply(&out);
    assert_screen_matches(&model, &first);

    let mut second = CellBuffer::new(8, 1, Rgb::BLACK);
    second.put(0, 0, Grapheme::new("漢"), Rgb::WHITE, Attrs::NONE);
    second.put(0, 0, Grapheme::from('a'), Rgb::WHITE, Attrs::NONE);
    second.put(2, 0, Grapheme::from('x'), Rgb::WHITE, Attrs::NONE);
    out.clear();
    second.diff_into(&first, ColorDepth::TrueColor, &mut out);
    model.apply(&out);
    assert_screen_matches(&model, &second);
    let row = model.row(0);
    assert_eq!(&row[..3], &["a", " ", "x"]);
}

#[test]
fn a_narrow_glyph_over_a_wide_tail_blanks_the_head() {
    let mut model = TerminalModel::new(8, 1);
    let empty = CellBuffer::new(0, 0, Rgb::BLACK);

    let mut first = CellBuffer::new(8, 1, Rgb::BLACK);
    first.put(0, 0, Grapheme::new("漢"), Rgb::WHITE, Attrs::NONE);
    first.put(2, 0, Grapheme::from('x'), Rgb::WHITE, Attrs::NONE);
    let mut out = Vec::new();
    first.diff_into(&empty, ColorDepth::TrueColor, &mut out);
    model.apply(&out);
    assert_screen_matches(&model, &first);

    let mut second = CellBuffer::new(8, 1, Rgb::BLACK);
    second.put(0, 0, Grapheme::new("漢"), Rgb::WHITE, Attrs::NONE);
    second.put(1, 0, Grapheme::from('b'), Rgb::WHITE, Attrs::NONE);
    second.put(2, 0, Grapheme::from('x'), Rgb::WHITE, Attrs::NONE);
    out.clear();
    second.diff_into(&first, ColorDepth::TrueColor, &mut out);
    model.apply(&out);
    assert_screen_matches(&model, &second);
    let row = model.row(0);
    assert_eq!(&row[..3], &[" ", "b", "x"]);
}
