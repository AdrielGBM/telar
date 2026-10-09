//! The writing direction, and the logical-to-physical edge mapping it decides.

/// The writing direction the layout resolves logical edges against.
///
/// Layout is authored in *logical* terms — start/end rather than left/right — and resolved to physical edges when a style is handed to the engine. One build therefore serves both directions: flipping [`Direction`] re-resolves the tree in place instead of rebuilding it, which is why the intent is kept alongside the resolved style rather than baked into it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Direction {
    #[default]
    Ltr,
    Rtl,
}

impl Direction {
    pub fn is_rtl(self) -> bool {
        matches!(self, Direction::Rtl)
    }

    /// The direction conventionally written with `locale`'s script, keyed by language subtag: Arabic, Hebrew, Persian, Urdu and the other right-to-left languages, matched against the tag's primary subtag so `ar-EG` resolves like `ar`.
    pub fn for_locale(locale: &str) -> Self {
        let lang = locale
            .split(['-', '_'])
            .next()
            .unwrap_or(locale)
            .to_ascii_lowercase();
        const RTL: &[&str] = &[
            "ar", "arc", "ckb", "dv", "fa", "ha", "he", "khw", "ks", "ps", "sd", "ur", "uz", "yi",
        ];
        if RTL.contains(&lang.as_str()) {
            Direction::Rtl
        } else {
            Direction::Ltr
        }
    }
}

/// Taffy is told the direction on every node rather than only having the main axis of a row reversed for it: alignment on a column's cross axis, a block's placement of a narrower child and a grid's columns all follow the inline direction too, and only taffy sees all of them.
impl From<Direction> for taffy::Direction {
    fn from(direction: Direction) -> Self {
        match direction {
            Direction::Ltr => taffy::Direction::Ltr,
            Direction::Rtl => taffy::Direction::Rtl,
        }
    }
}

#[cfg(test)]
#[path = "direction_test.rs"]
mod tests;
