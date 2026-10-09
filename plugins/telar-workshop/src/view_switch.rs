//! The view switch in the top bar: tabs choosing what the area beside the sidebar shows.

use telar::{Color, LayoutError, LayoutItem};
use telar_components::{Tab, tab_list};

use crate::state::{ViewMode, WorkshopState};
use crate::strings::{self, VIEW, VIEW_CANVAS, VIEW_DOCS};

/// The views offered.
pub(crate) fn view_switch(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    tab_list(
        state.view(),
        vec![
            Tab::new(ViewMode::Canvas, strings::reactive(VIEW_CANVAS)),
            Tab::new(ViewMode::Docs, strings::reactive(VIEW_DOCS)),
        ],
        Color::TRANSPARENT.into(),
        Some(strings::reactive(VIEW)),
    )
}

#[cfg(test)]
#[path = "view_switch_test.rs"]
mod tests;
