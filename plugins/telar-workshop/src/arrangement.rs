//! How the chrome arranges itself for the width of the window it is in: the regions it keeps in place as panes, and the ones it folds into a drawer when the window is too narrow for them.

use telar::{Memo, breakpoint};

/// The narrowest window that shows the sidebar as a pane beside the canvas.
pub(crate) const PANES_FROM: f32 = 900.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Arrangement {
    /// The sidebar opens as a drawer over the workshop, and the panels sit below the canvas wherever they were put.
    Compact,
    /// Every region in place.
    Wide,
}

impl Arrangement {
    /// The arrangement for the width of the surface this is called on, re-chosen as that width crosses [`PANES_FROM`]. A surface given no size yet is arranged as a wide one: folding regions away for a width nobody has reported would only unfold them, remounting the canvas, once it is.
    pub(crate) fn follow() -> Memo<Self> {
        breakpoint(Self::Wide)
            .at(f32::MIN_POSITIVE, Self::Compact)
            .at(PANES_FROM, Self::Wide)
            .follow()
    }

    pub(crate) fn sidebar_in_drawer(self) -> bool {
        self == Self::Compact
    }
}

/// The widest the bar's start group gets, its sidebar opener included, and the widest its end group gets, the window's own controls included, with the bar's padding.
const START_GROUP: f32 = 260.0;
const END_GROUP: f32 = 220.0;
/// The narrowest the palette's field is worth drawing at: narrower, it cannot show a project's name beside its icon.
pub(crate) const FIND_FIELD_COMFORT: f32 = 200.0;
const BAR_GAP: f32 = 16.0;
/// The narrowest window whose bar keeps the palette's field centred between its groups, each holding a share as wide as the wider group needs.
pub(crate) const FIND_FIELD_FROM: f32 =
    2.0 * START_GROUP.max(END_GROUP) + 2.0 * BAR_GAP + FIND_FIELD_COMFORT;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum FindPlacement {
    /// The palette's trigger is a field in the middle of the bar.
    Field,
    /// The bar has no room for a field, so the trigger is an icon button among the end group's.
    Button,
}

impl FindPlacement {
    /// The placement for the width of the surface this is called on, re-chosen as that width crosses [`FIND_FIELD_FROM`]. A surface given no size yet is given the field, as [`Arrangement::follow`] gives it the wide arrangement.
    pub(crate) fn follow() -> Memo<Self> {
        breakpoint(Self::Field)
            .at(f32::MIN_POSITIVE, Self::Button)
            .at(FIND_FIELD_FROM, Self::Field)
            .follow()
    }
}
