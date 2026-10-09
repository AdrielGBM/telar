//! [`WorkshopApp`]: the workshop as the application a preview build runs.

use std::cell::RefCell;
use std::rc::Rc;

use telar::preview::PreviewEntry;
use telar::preview::host::{PreviewRequest, duplicate_ids, requested_preview};
use telar::{
    App, Color, Component, LayoutError, LayoutItem, LayoutStyle, ReactiveList, RectStyle,
    ShapeStyle, SizeDimension, StyledContainer, WindowRoot, box_item, reset_layout_runtime,
    use_resolved_scheme,
};
use telar_devtools::{
    WORKBENCH_CONTROL_SIZE, WorkbenchTheme, use_workbench_tokens, workbench_scope,
};

use crate::canvas::env::follow_reduced_motion;
use crate::shell::shell;
use crate::state::WorkshopState;
use crate::store::Keeper;
use crate::{address, canvas, store};

/// The component workshop: a sidebar listing `entries`, a canvas rendering the selected one, and panels to edit its args.
///
/// It opens where the location it was started at points (`--location`, a link the workshop copied), else on the preview [`requested_preview`] names by id, else where it was when it last closed, else on the first preview; the component the request names narrows the sidebar. After a hot reload it stays where it was. A `goto:` line on the hot-reload channel moves it while it runs.
///
/// Under `cargo telar preview` it keeps the selection, the view, the pane sizes and the canvas toolbar's settings between runs in `<workspace>/.telar/workshop/state`; edited args last the run, or travel in a copied link.
pub struct WorkshopApp {
    entries: Rc<[PreviewEntry]>,
    requested: PreviewRequest,
    project_name: Option<String>,
    keeper: RefCell<Keeper>,
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
            keeper: RefCell::new(Keeper::none()),
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
        let keeper = store::install();
        self.keeper.replace(keeper.clone()).flush();
        let state = WorkshopState::kept_in(Rc::clone(&self.entries), &self.requested, &keeper);
        follow_reduced_motion(&state);
        address::follow(&state);
        let project_name = self.project_name.clone();
        let chrome = workbench_scope(|| workshop(&state, project_name))
            .expect("the workshop's chrome failed to build");
        Box::new(
            WindowRoot::wrapping(Box::new(chrome)).expect("the workshop's window failed to build"),
        )
    }

    fn clear_color(&self) -> Option<Color> {
        Some(WorkbenchTheme::for_scheme(use_resolved_scheme()).surface)
    }
}

impl Drop for WorkshopApp {
    fn drop(&mut self) {
        self.keeper.get_mut().flush();
    }
}

/// The whole workshop, or the canvas alone while its address asks for no chrome.
fn workshop(
    state: &WorkshopState,
    project_name: Option<String>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let chrome = state.chrome();
    let building = state.clone();
    let shown = ReactiveList::with_style(
        LayoutStyle::new()
            .flex_column()
            .width(SizeDimension::Percent(1.0))
            .height(SizeDimension::Percent(1.0)),
        move || vec![chrome.get()],
        |chrome: &bool| *chrome,
        move |chrome| match chrome {
            true => shell(&building, project_name.clone()),
            false => canvas_alone(&building),
        },
    )?;
    Ok(box_item(shown))
}

fn canvas_alone(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let pane = StyledContainer::new(
        LayoutStyle::new()
            .flex_column()
            .flex_grow(1.0)
            .min_width(0.0)
            .min_height(0.0),
        |_| RectStyle::default().with_fill(use_workbench_tokens().canvas_backdrop),
        vec![canvas::bare(state)?],
    )?;
    Ok(box_item(pane))
}

#[cfg(test)]
#[path = "app_test.rs"]
mod tests;
