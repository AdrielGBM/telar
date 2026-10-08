//! [`WorkshopApp`]: the workshop as the application a preview build runs.

use std::rc::Rc;

use telar::preview::PreviewEntry;
use telar::preview::host::{PreviewRequest, duplicate_ids, requested_preview};
use telar::{App, Color, Component, WindowRoot, reset_layout_runtime, use_resolved_scheme};
use telar_devtools::{WORKBENCH_CONTROL_SIZE, WorkbenchTheme, workbench_scope};

use crate::shell::shell;
use crate::state::WorkshopState;

/// The component workshop: a sidebar listing `entries`, a canvas rendering the selected one, and panels to edit its args.
///
/// It opens on the preview [`requested_preview`] names by id, else on the first preview of the component it names, else on the first preview; after a hot reload it stays on the preview it showed.
pub struct WorkshopApp {
    entries: Rc<[PreviewEntry]>,
    requested: PreviewRequest,
    project_name: Option<String>,
}

impl WorkshopApp {
    /// The workshop for `entries`, in the order they are listed. A debug build panics when two of them share an id, naming where each is written.
    pub fn new(entries: Vec<PreviewEntry>) -> Self {
        if cfg!(debug_assertions)
            && let Some(duplicates) = duplicate_ids(&entries)
        {
            panic!("{duplicates}");
        }
        Self {
            entries: entries.into(),
            requested: requested_preview(),
            project_name: std::env::var("CARGO_PKG_NAME")
                .ok()
                .filter(|name| !name.is_empty()),
        }
    }

    /// Opens on the preview `id` names, whatever the environment asked for. An id no preview has is ignored.
    pub fn select(mut self, id: impl Into<String>) -> Self {
        self.requested.id = Some(id.into());
        self
    }

    /// The name the top bar shows. Defaults to the package `cargo` runs.
    pub fn project_name(mut self, name: impl Into<String>) -> Self {
        self.project_name = Some(name.into());
        self
    }
}

impl App for WorkshopApp {
    fn root(&self) -> Box<dyn Component> {
        reset_layout_runtime();
        // The window's own surface, not the thread: a canvas inside keeps the application's control size.
        theme_core::set_surface_control_size(Some(WORKBENCH_CONTROL_SIZE));
        let state = WorkshopState::new(Rc::clone(&self.entries), &self.requested);
        let project_name = self.project_name.clone();
        let chrome = workbench_scope(|| shell(&state, project_name))
            .expect("the workshop's chrome failed to build");
        Box::new(
            WindowRoot::wrapping(Box::new(chrome)).expect("the workshop's window failed to build"),
        )
    }

    fn clear_color(&self) -> Option<Color> {
        Some(WorkbenchTheme::for_scheme(use_resolved_scheme()).surface)
    }
}

#[cfg(test)]
#[path = "app_test.rs"]
mod tests;
