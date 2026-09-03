//! What one character cell holds.

use crate::color::Rgb;

/// A grapheme cluster stored inline. Long enough for every cluster a UI realistically renders — a
/// four-person family emoji is 25 bytes and fits — so a cell never allocates and a buffer is one flat
/// `Vec`. A longer cluster keeps its base character and drops the rest, which is what a terminal that
/// cannot compose it would show anyway.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Grapheme {
    buf: [u8; 27],
    len: u8,
}

impl Grapheme {
    pub const SPACE: Self = Self::from_ascii(b' ');

    const fn from_ascii(b: u8) -> Self {
        let mut buf = [0u8; 27];
        buf[0] = b;
        Self { buf, len: 1 }
    }

    pub fn new(s: &str) -> Self {
        let bytes = s.as_bytes();
        if bytes.len() <= 27 {
            let mut buf = [0u8; 27];
            buf[..bytes.len()].copy_from_slice(bytes);
            return Self {
                buf,
                len: bytes.len() as u8,
            };
        }
        match s.chars().next() {
            Some(c) => {
                let mut buf = [0u8; 27];
                let n = c.encode_utf8(&mut buf).len();
                Self { buf, len: n as u8 }
            }
            None => Self::SPACE,
        }
    }

    pub fn as_str(&self) -> &str {
        // The buffer only ever receives whole `str` slices or a whole encoded `char`.
        std::str::from_utf8(&self.buf[..self.len as usize]).unwrap_or(" ")
    }
}

impl Default for Grapheme {
    fn default() -> Self {
        Self::SPACE
    }
}

impl From<char> for Grapheme {
    fn from(c: char) -> Self {
        let mut buf = [0u8; 27];
        let n = c.encode_utf8(&mut buf).len();
        Self { buf, len: n as u8 }
    }
}

/// The text attributes a terminal can carry that Telar's text style can ask for. Deliberately not the
/// terminal's full SGR vocabulary: an attribute nothing upstream can express is one nothing can test.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Attrs(u8);

impl Attrs {
    pub const BOLD: Self = Self(1 << 0);
    pub const ITALIC: Self = Self(1 << 1);
    pub const DIM: Self = Self(1 << 2);
    /// The trailing column of a double-width grapheme. It draws nothing: the wide cell to its left already
    /// covers it, and writing anything here would push the row out of alignment.
    pub const WIDE_TAIL: Self = Self(1 << 3);

    pub const NONE: Self = Self(0);

    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn bits(self) -> u8 {
        self.0
    }

    pub const fn from_bits(bits: u8) -> Self {
        Self(bits)
    }
}

impl std::ops::BitOr for Attrs {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        self.with(rhs)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Cell {
    pub glyph: Grapheme,
    pub fg: Rgb,
    pub bg: Rgb,
    pub attrs: Attrs,
}

impl Cell {
    pub fn blank(bg: Rgb) -> Self {
        Self {
            glyph: Grapheme::SPACE,
            fg: Rgb::WHITE,
            bg,
            attrs: Attrs::NONE,
        }
    }

    /// Whether this cell draws nothing but its background — so a writer can skip its foreground colour
    /// entirely, which is most of a UI.
    pub fn is_blank(&self) -> bool {
        self.attrs.contains(Attrs::WIDE_TAIL) || self.glyph == Grapheme::SPACE
    }
}

impl Default for Cell {
    fn default() -> Self {
        Self::blank(Rgb::BLACK)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holds_a_multi_byte_cluster() {
        let g = Grapheme::new("é");
        assert_eq!(g.as_str(), "é");
    }

    #[test]
    fn holds_a_zwj_sequence() {
        let family = "👨‍👩‍👧‍👦";
        assert!(
            family.len() <= 27,
            "fixture must fit inline: {}",
            family.len()
        );
        assert_eq!(Grapheme::new(family).as_str(), family);
    }

    #[test]
    fn overlong_cluster_keeps_its_base_character() {
        let long = "a\u{0301}\u{0302}\u{0303}\u{0304}\u{0305}\u{0306}\u{0307}\u{0308}\u{0309}\u{030a}\u{030b}\u{030c}\u{030d}\u{030e}";
        assert!(long.len() > 27);
        assert_eq!(Grapheme::new(long).as_str(), "a");
    }

    #[test]
    fn attrs_compose() {
        let a = Attrs::BOLD | Attrs::ITALIC;
        assert!(a.contains(Attrs::BOLD));
        assert!(a.contains(Attrs::ITALIC));
        assert!(!a.contains(Attrs::DIM));
    }
}
