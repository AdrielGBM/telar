//! Which keys a focused box keeps for itself.
//!
//! A key a control does not use still means something to whatever hosts the surface: Tab moves focus through a document, the arrows and Space scroll it. The declaration lets a target hand exactly those keys back instead of guessing per key for the whole surface. Only keys some host gives a default action to are named; no host scrolls or moves focus with a character.

use crate::Role;

/// A set of the keys a host acts on by default, as the focused box declares it keeps them.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct ConsumedKeys(u16);

impl ConsumedKeys {
    pub const EMPTY: Self = Self(0);
    pub const TAB: Self = Self(1 << 0);
    pub const SPACE: Self = Self(1 << 1);
    pub const ENTER: Self = Self(1 << 2);
    pub const BACKSPACE: Self = Self(1 << 3);
    pub const ARROW_UP: Self = Self(1 << 4);
    pub const ARROW_DOWN: Self = Self(1 << 5);
    pub const ARROW_LEFT: Self = Self(1 << 6);
    pub const ARROW_RIGHT: Self = Self(1 << 7);
    pub const PAGE_UP: Self = Self(1 << 8);
    pub const PAGE_DOWN: Self = Self(1 << 9);
    pub const HOME: Self = Self(1 << 10);
    pub const END: Self = Self(1 << 11);

    pub const VERTICAL_ARROWS: Self = Self::ARROW_UP.with(Self::ARROW_DOWN);
    pub const HORIZONTAL_ARROWS: Self = Self::ARROW_LEFT.with(Self::ARROW_RIGHT);
    pub const ARROWS: Self = Self::VERTICAL_ARROWS.with(Self::HORIZONTAL_ARROWS);
    pub const PAGING: Self = Self::PAGE_UP.with(Self::PAGE_DOWN);
    pub const EDGES: Self = Self::HOME.with(Self::END);
    /// What pressing a control means: the two keys that do what a tap does.
    pub const ACTIVATION: Self = Self::SPACE.with(Self::ENTER);
    /// Everything a host scrolls with.
    pub const SCROLLING: Self = Self::ARROWS
        .with(Self::PAGING)
        .with(Self::EDGES)
        .with(Self::SPACE);

    const NAMES: [(Self, &'static str); 12] = [
        (Self::TAB, "tab"),
        (Self::SPACE, "space"),
        (Self::ENTER, "enter"),
        (Self::BACKSPACE, "backspace"),
        (Self::ARROW_UP, "up"),
        (Self::ARROW_DOWN, "down"),
        (Self::ARROW_LEFT, "left"),
        (Self::ARROW_RIGHT, "right"),
        (Self::PAGE_UP, "pageup"),
        (Self::PAGE_DOWN, "pagedown"),
        (Self::HOME, "home"),
        (Self::END, "end"),
    ];

    pub const fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn without(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The set spelled as space-separated key names, the form a target that has to write it down stores it in.
    pub fn to_names(self) -> String {
        Self::NAMES
            .iter()
            .filter(|(key, _)| self.contains(*key))
            .map(|(_, name)| *name)
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// The set [`to_names`](Self::to_names) wrote, or an author spelled. A name it does not know is skipped rather than failing the rest, so an older reader still honours the keys it understands.
    pub fn from_names(names: &str) -> Self {
        names
            .split_ascii_whitespace()
            .filter_map(Self::named)
            .fold(Self::EMPTY, Self::with)
    }

    /// The keys one name stands for: a single key as [`to_names`](Self::to_names) spells it, or one of the groups an author reaches for (`arrows`, `vertical-arrows`, `horizontal-arrows`, `activation`, `paging`, `edges`, `scrolling`, `none`).
    pub fn named(word: &str) -> Option<Self> {
        Self::NAMES
            .iter()
            .chain(Self::GROUPS.iter())
            .find(|(_, name)| *name == word)
            .map(|(key, _)| *key)
    }

    const GROUPS: [(Self, &'static str); 8] = [
        (Self::ARROWS, "arrows"),
        (Self::VERTICAL_ARROWS, "vertical-arrows"),
        (Self::HORIZONTAL_ARROWS, "horizontal-arrows"),
        (Self::ACTIVATION, "activation"),
        (Self::PAGING, "paging"),
        (Self::EDGES, "edges"),
        (Self::SCROLLING, "scrolling"),
        (Self::EMPTY, "none"),
    ];
}

impl std::ops::BitOr for ConsumedKeys {
    type Output = Self;

    fn bitor(self, other: Self) -> Self {
        self.with(other)
    }
}

impl Role {
    /// The keys a focused, enabled control of this role keeps, as Telar's own controls of that role use them.
    ///
    /// A widget whose keys depend on its state — a dropdown that only walks rows while open — declares its own set instead; this is the answer for the ones that said nothing. Tab is kept only by the multi-line editor, which types it: every other control leaves it to move focus.
    pub fn consumed_keys(&self) -> ConsumedKeys {
        use ConsumedKeys as K;
        match self {
            Self::Button
            | Self::CheckBox
            | Self::Radio
            | Self::Switch
            | Self::Tab
            | Self::Disclosure => K::ACTIVATION,
            Self::MenuItem | Self::ComboBox => K::ACTIVATION | K::VERTICAL_ARROWS | K::EDGES,
            Self::Slider => K::ARROWS,
            Self::SpinButton => K::ARROWS | K::ENTER,
            Self::TextInput => K::ARROWS | K::EDGES | K::SPACE | K::ENTER | K::BACKSPACE,
            Self::MultilineTextInput => {
                K::ARROWS | K::EDGES | K::SPACE | K::ENTER | K::BACKSPACE | K::TAB
            }
            Self::ScrollArea => K::SCROLLING,
            Self::Group
            | Self::Banner
            | Self::Navigation
            | Self::Main
            | Self::Complementary
            | Self::ContentInfo
            | Self::Article
            | Self::Section
            | Self::Form
            | Self::Search
            | Self::Heading(_)
            | Self::List
            | Self::ListItem
            | Self::Drawing
            | Self::Dialog
            | Self::TabPanel
            | Self::ProgressBar
            | Self::Label => K::EMPTY,
            // A document activates a link on Enter by itself and scrolls on Space, and a link keeping either would take that from it.
            Self::Link => K::EMPTY,
        }
    }
}

#[cfg(test)]
#[path = "keys_test.rs"]
mod tests;
