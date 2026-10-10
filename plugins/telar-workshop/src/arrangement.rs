//! How the chrome arranges itself for the width of the window it is in: the regions it keeps in place as panes, and the ones it folds into a drawer or a menu when the window is too narrow for them.

use telar::{Memo, breakpoint};

/// The narrowest window that shows the sidebar as a pane beside the canvas.
pub(crate) const PANES_FROM: f32 = 900.0;
/// The narrowest window whose top bar holds every canvas setting in place.
pub(crate) const SETTINGS_FROM: f32 = 1280.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Arrangement {
    /// The sidebar opens as a drawer over the workshop, the panels sit below the canvas wherever they were put, and the canvas settings are a menu.
    Compact,
    /// The sidebar and the panels are panes, and the canvas settings a menu.
    Medium,
    /// Every region in place.
    Wide,
}

impl Arrangement {
    /// The arrangement for the width of the surface this is called on, re-chosen as that width crosses [`PANES_FROM`] or [`SETTINGS_FROM`]. A surface given no size yet is arranged as a wide one: folding regions away for a width nobody has reported would only unfold them, remounting the canvas, once it is.
    pub(crate) fn follow() -> Memo<Self> {
        breakpoint(Self::Wide)
            .at(f32::MIN_POSITIVE, Self::Compact)
            .at(PANES_FROM, Self::Medium)
            .at(SETTINGS_FROM, Self::Wide)
            .follow()
    }

    pub(crate) fn sidebar_in_drawer(self) -> bool {
        self == Self::Compact
    }

    pub(crate) fn settings_in_menu(self) -> bool {
        self != Self::Wide
    }
}
