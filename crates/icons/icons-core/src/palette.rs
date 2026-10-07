//! Whether an icon takes the colour around it or keeps its own, decided the Iconify way.

use crate::{SetInfo, SourcedIcon};

/// Whether an icon is drawn to take the colour of what surrounds it rather than colours of its own.
///
/// Iconify's rule: a set that declares `palette: false` is monochrome and one that declares `palette: true` is not, whatever its markup holds, so a brand logo drawn in one colour keeps it. A set that declares neither, such as a folder of an application's own SVGs, is judged by its markup: it is monochrome when it paints in `currentColor`, which is how every monochrome Iconify icon is written.
pub fn is_monochrome(set: Option<&SetInfo>, svg: &str) -> bool {
    match set.and_then(|info| info.palette) {
        Some(palette) => !palette,
        None => uses_current_color(svg),
    }
}

/// Whether `svg` paints anything in `currentColor`. CSS keywords ignore case, so `currentcolor` counts too.
pub fn uses_current_color(svg: &str) -> bool {
    const KEYWORD: &[u8] = b"currentcolor";
    svg.as_bytes()
        .windows(KEYWORD.len())
        .any(|window| window.eq_ignore_ascii_case(KEYWORD))
}

impl SourcedIcon {
    /// Whether this icon takes the colour around it; see [`is_monochrome`].
    pub fn monochrome(&self) -> bool {
        is_monochrome(self.set.as_ref(), &self.svg)
    }
}

#[cfg(test)]
#[path = "palette_test.rs"]
mod tests;
