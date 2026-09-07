//! The cell grid, and turning one frame's difference from the last into terminal output.

use unicode_width::UnicodeWidthStr;

use crate::cell::{Attrs, Cell, Grapheme};
use crate::color::{ColorDepth, Rgb, to_ansi16, to_ansi256};

/// The cell grid one frame is painted into, and diffed against the last.
pub struct CellBuffer {
    cols: u16,
    rows: u16,
    cells: Vec<Cell>,
}

impl CellBuffer {
    pub fn new(cols: u16, rows: u16, bg: Rgb) -> Self {
        Self {
            cols,
            rows,
            cells: vec![Cell::blank(bg); cols as usize * rows as usize],
        }
    }

    pub fn cols(&self) -> u16 {
        self.cols
    }

    pub fn rows(&self) -> u16 {
        self.rows
    }

    /// Resizes to `cols`×`rows`, discarding the old contents. The caller repaints the frame anyway, and carrying pixels across a resize is what leaves a stale column down the edge of the screen.
    pub fn resize(&mut self, cols: u16, rows: u16, bg: Rgb) {
        self.cols = cols;
        self.rows = rows;
        self.cells.clear();
        self.cells
            .resize(cols as usize * rows as usize, Cell::blank(bg));
    }

    pub fn clear(&mut self, bg: Rgb) {
        let blank = Cell::blank(bg);
        self.cells.fill(blank);
    }

    fn index(&self, col: u16, row: u16) -> Option<usize> {
        (col < self.cols && row < self.rows)
            .then(|| row as usize * self.cols as usize + col as usize)
    }

    pub fn get(&self, col: u16, row: u16) -> Option<&Cell> {
        self.index(col, row).map(|i| &self.cells[i])
    }

    pub fn get_mut(&mut self, col: u16, row: u16) -> Option<&mut Cell> {
        self.index(col, row).map(|i| &mut self.cells[i])
    }

    /// Writes a grapheme at `col`, claiming the next column too when the terminal will render it two cells wide. Returns how many columns it took, so a caller laying out a run advances correctly.
    pub fn put(&mut self, col: u16, row: u16, glyph: Grapheme, fg: Rgb, attrs: Attrs) -> u16 {
        let width = glyph.as_str().width().max(1) as u16;
        let Some(i) = self.index(col, row) else {
            return width;
        };
        // A wide glyph with only one column left would spill into the next row; a space keeps the grid honest.
        if width == 2 && col + 1 >= self.cols {
            self.cells[i].glyph = Grapheme::SPACE;
            return width;
        }
        self.cells[i].glyph = glyph;
        self.cells[i].fg = fg;
        self.cells[i].attrs = attrs;
        if width == 2 {
            let tail = i + 1;
            self.cells[tail].glyph = Grapheme::SPACE;
            self.cells[tail].fg = fg;
            self.cells[tail].attrs = attrs.with(Attrs::WIDE_TAIL);
            self.cells[tail].bg = self.cells[i].bg;
        }
        width
    }

    /// Whether the cell at `col` is the leading half of a double-width grapheme.
    fn is_wide_head(&self, col: u16, row: u16) -> bool {
        self.get(col + 1, row)
            .is_some_and(|c| c.attrs.contains(Attrs::WIDE_TAIL))
    }

    /// Appends the escape sequences that turn `previous` into `self`. An unchanged frame appends nothing.
    pub fn diff_into(&self, previous: &CellBuffer, depth: ColorDepth, out: &mut Vec<u8>) {
        let sized_alike = previous.cols == self.cols && previous.rows == self.rows;
        let mut pen = Pen::unset();
        let mut cursor: Option<(u16, u16)> = None;

        for row in 0..self.rows {
            let mut col = 0;
            while col < self.cols {
                let i = row as usize * self.cols as usize + col as usize;
                let cell = &self.cells[i];
                if cell.attrs.contains(Attrs::WIDE_TAIL) {
                    col += 1;
                    continue;
                }
                let wide = self.is_wide_head(col, row);
                let span = if wide { 2 } else { 1 };
                let unchanged = sized_alike
                    && (0..span).all(|k| {
                        previous
                            .get(col + k, row)
                            .is_some_and(|p| p == &self.cells[i + k as usize])
                    });
                if unchanged {
                    col += span;
                    continue;
                }
                if cursor != Some((col, row)) {
                    write_move(out, col, row);
                }
                pen.apply(cell, depth, out);
                out.extend_from_slice(cell.glyph.as_str().as_bytes());
                cursor = Some((col + span, row));
                col += span;
            }
        }
        if !out.is_empty() {
            out.extend_from_slice(b"\x1b[0m");
        }
    }
}

fn write_move(out: &mut Vec<u8>, col: u16, row: u16) {
    out.extend_from_slice(b"\x1b[");
    write_u16(out, row + 1);
    out.push(b';');
    write_u16(out, col + 1);
    out.push(b'H');
}

fn write_u16(out: &mut Vec<u8>, mut v: u16) {
    let mut digits = [0u8; 5];
    let mut n = 0;
    loop {
        digits[n] = b'0' + (v % 10) as u8;
        v /= 10;
        n += 1;
        if v == 0 {
            break;
        }
    }
    for i in (0..n).rev() {
        out.push(digits[i]);
    }
}

/// The terminal's current graphic state, so a frame only says what changed.
struct Pen {
    fg: Option<Rgb>,
    bg: Option<Rgb>,
    attrs: Option<Attrs>,
}

impl Pen {
    fn unset() -> Self {
        Self {
            fg: None,
            bg: None,
            attrs: None,
        }
    }

    fn apply(&mut self, cell: &Cell, depth: ColorDepth, out: &mut Vec<u8>) {
        let attrs = Attrs::from_bits(cell.attrs.bits() & !Attrs::WIDE_TAIL.bits());
        // Turning an attribute *off* has no SGR of its own that is safe across terminals, so the whole pen is reset and restated. Turning one on is additive and costs one parameter.
        let clears = self
            .attrs
            .is_some_and(|current| current.bits() & !attrs.bits() != 0);
        if clears || self.attrs.is_none() {
            out.extend_from_slice(b"\x1b[0m");
            self.fg = None;
            self.bg = None;
            self.attrs = Some(Attrs::NONE);
        }
        let current = self.attrs.unwrap_or(Attrs::NONE);
        for (flag, code) in [
            (Attrs::BOLD, b"\x1b[1m".as_slice()),
            (Attrs::DIM, b"\x1b[2m".as_slice()),
            (Attrs::ITALIC, b"\x1b[3m".as_slice()),
        ] {
            if attrs.contains(flag) && !current.contains(flag) {
                out.extend_from_slice(code);
            }
        }
        self.attrs = Some(attrs);

        if self.bg != Some(cell.bg) {
            write_color(out, cell.bg, false, depth);
            self.bg = Some(cell.bg);
        }
        // A blank cell paints only its background, so its foreground is not worth a sequence.
        if !cell.is_blank() && self.fg != Some(cell.fg) {
            write_color(out, cell.fg, true, depth);
            self.fg = Some(cell.fg);
        }
    }
}

fn write_color(out: &mut Vec<u8>, c: Rgb, foreground: bool, depth: ColorDepth) {
    match depth {
        ColorDepth::TrueColor => {
            out.extend_from_slice(if foreground {
                b"\x1b[38;2;"
            } else {
                b"\x1b[48;2;"
            });
            write_u16(out, c.r as u16);
            out.push(b';');
            write_u16(out, c.g as u16);
            out.push(b';');
            write_u16(out, c.b as u16);
            out.push(b'm');
        }
        ColorDepth::Ansi256 => {
            out.extend_from_slice(if foreground {
                b"\x1b[38;5;"
            } else {
                b"\x1b[48;5;"
            });
            write_u16(out, to_ansi256(c) as u16);
            out.push(b'm');
        }
        ColorDepth::Ansi16 => {
            let idx = to_ansi16(c);
            // 30-37 / 40-47 for the first eight, 90-97 / 100-107 for the bright half.
            let base = match (foreground, idx < 8) {
                (true, true) => 30,
                (true, false) => 82,
                (false, true) => 40,
                (false, false) => 92,
            };
            out.extend_from_slice(b"\x1b[");
            write_u16(out, base as u16 + idx as u16);
            out.push(b'm');
        }
    }
}

#[cfg(test)]
#[path = "buffer_test.rs"]
mod tests;

/// A model of what a terminal ends up showing, built by replaying the bytes [`CellBuffer::diff_into`] wrote.
///
/// The diff's whole contract is "apply this to the previous frame and you have the new one", and nothing short of interpreting the output actually checks it: comparing two buffers proves they differ, not that the escape sequences between them are right.
#[cfg(test)]
pub(crate) struct TerminalModel {
    cols: u16,
    rows: u16,
    glyphs: Vec<String>,
    cursor: (u16, u16),
}

#[cfg(test)]
impl TerminalModel {
    pub(crate) fn new(cols: u16, rows: u16) -> Self {
        Self {
            cols,
            rows,
            glyphs: vec![" ".to_string(); cols as usize * rows as usize],
            cursor: (0, 0),
        }
    }

    pub(crate) fn apply(&mut self, bytes: &[u8]) {
        let text = std::str::from_utf8(bytes).expect("the writer only emits UTF-8");
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            if c != '\x1b' {
                self.put(c);
                continue;
            }
            // Every escape this writer emits is either a CSI or a one-character sequence; only the CSIs that move the cursor change what ends up on the screen.
            if chars.next() == Some('[') {
                let mut params = String::new();
                let final_byte = loop {
                    match chars.next() {
                        Some(c) if c.is_ascii_alphabetic() => break c,
                        Some(c) => params.push(c),
                        None => return,
                    }
                };
                if final_byte == 'H' {
                    let mut parts = params.split(';');
                    let row: u16 = parts.next().unwrap_or("1").parse().unwrap_or(1);
                    let col: u16 = parts.next().unwrap_or("1").parse().unwrap_or(1);
                    self.cursor = (col.saturating_sub(1), row.saturating_sub(1));
                }
            }
        }
    }

    fn put(&mut self, c: char) {
        let (col, row) = self.cursor;
        let width = unicode_width::UnicodeWidthChar::width(c)
            .unwrap_or(1)
            .max(1) as u16;
        if col < self.cols && row < self.rows {
            self.glyphs[row as usize * self.cols as usize + col as usize] = c.to_string();
            // A double-width glyph covers the next column outright: whatever was there is gone, and the column contributes nothing of its own to the row.
            if width == 2 && col + 1 < self.cols {
                self.glyphs[row as usize * self.cols as usize + col as usize + 1] = String::new();
            }
        }
        self.cursor = (col + width, row);
    }

    /// The screen as one string per row, for an assertion that reads like the screen.
    pub(crate) fn row(&self, row: u16) -> String {
        (0..self.cols)
            .map(|c| self.glyphs[row as usize * self.cols as usize + c as usize].as_str())
            .collect()
    }
}

#[cfg(test)]
#[path = "buffer_replay_test.rs"]
mod replay_tests;
