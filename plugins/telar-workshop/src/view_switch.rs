//! The view switch in the top bar: tabs choosing what the area beside the sidebar shows.

use telar::{Color, LayoutError, LayoutItem};
use telar_components::{Tab, tab_list};

use crate::state::{ViewMode, WorkshopState};
use crate::strings::{self, VIEW, VIEW_CANVAS, VIEW_DOCS, VIEW_MATRIX};

/// The views offered, in the order the tabs and Alt+1, Alt+2 and Alt+3 give them, each with its [`crate::strings`] key.
pub(crate) const VIEWS: [(ViewMode, &str); 3] = [
    (ViewMode::Canvas, VIEW_CANVAS),
    (ViewMode::Matrix, VIEW_MATRIX),
    (ViewMode::Docs, VIEW_DOCS),
];

pub(crate) fn view_switch(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    tab_list(
        state.view(),
        VIEWS
            .into_iter()
            .map(|(view, key)| Tab::new(view, strings::reactive(key)))
            .collect(),
        Color::TRANSPARENT.into(),
        Some(strings::reactive(VIEW)),
    )
}

/// What `view` is called, as a [`crate::strings`] key.
pub(crate) fn view_key(view: ViewMode) -> &'static str {
    VIEWS
        .into_iter()
        .find(|(listed, _)| *listed == view)
        .map_or(VIEW_CANVAS, |(_, key)| key)
}

#[cfg(test)]
#[path = "view_switch_test.rs"]
mod tests;
