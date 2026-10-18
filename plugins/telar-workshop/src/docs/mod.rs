//! The Docs view: a page per component, written from what its previews carry.
//!
//! The page is the selected preview's component, as the sidebar lists it by package and title. It opens with the prose its previews carry and the props' own doc, shows the component's first preview with its controls and source, lists the props, and then shows every other preview over its source, each mounted the first time it scrolls into view. The previews' callbacks log their calls in the workshop's one action log, which each page empties as it opens.

telar::rsx_modules!();

mod frame;

use telar::preview::PreviewEntry;
use telar::{
    AlignItems, Children, Container, JustifyContent, LayoutError, LayoutItem, ReactiveList, Role,
    Text, box_item, children,
};
use telar_devtools::{WORKBENCH_GRID, workbench_fill, workbench_muted};

use crate::panels::arg_table;
use crate::state::WorkshopState;
use crate::strings::{self, NO_PREVIEWS};

const PAGE_MARGIN: f32 = WORKBENCH_GRID * 3.0;
const PAGE_MAX_WIDTH: f32 = 960.0;
const TITLE_SIZE: f32 = 22.0;
const SECTION_SIZE: f32 = 16.0;

/// What fills the canvas area while the workshop shows docs.
pub(crate) fn docs(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    if state.entries().is_empty() {
        return message(NO_PREVIEWS);
    }
    let selected = state.clone();
    let building = state.clone();
    let docs = ReactiveList::with_style(
        workbench_fill(),
        move || {
            selected
                .selected()
                .map(|entry| (entry.package, entry.title))
                .into_iter()
                .collect()
        },
        |component: &(&'static str, &'static str)| *component,
        move |(package, title)| page(&building, package, title),
    )?;
    Ok(box_item(docs))
}

/// The page of the component `package` lists under `title`, which empties the action log as it opens.
fn page(
    state: &WorkshopState,
    package: &'static str,
    title: &'static str,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let entries: Vec<PreviewEntry> = state
        .entries()
        .iter()
        .filter(|entry| entry.package == package && entry.title == title)
        .copied()
        .collect();
    if entries.is_empty() {
        return message(NO_PREVIEWS);
    }
    state.actions().clear();
    page::page(
        page::PageProps::props()
            .state(state.clone())
            .title(title)
            .entries(entries)
            .build(),
        Children::default(),
    )
}

#[telar::component]
fn preview_controls(
    state: WorkshopState,
    entry: PreviewEntry,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    arg_table(state.args(&entry))
}

/// `text` as paragraphs, one per run of lines between blank ones, each run's lines joined as one wrapped line.
fn paragraphs(text: &str) -> Vec<String> {
    let mut paragraphs = Vec::new();
    let mut lines: Vec<&str> = Vec::new();
    for line in text.lines().map(str::trim).chain([""]) {
        if !line.is_empty() {
            lines.push(line);
        } else if !lines.is_empty() {
            paragraphs.push(lines.join(" "));
            lines.clear();
        }
    }
    paragraphs
}

fn message(key: &'static str) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let message = Text::declaring(
        move || strings::text(key),
        telar::LayoutStyle::new(),
        workbench_muted,
    )?;
    let centred = Container::new(
        workbench_fill()
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER),
        children![message],
    )?
    .role(Role::Status);
    Ok(box_item(centred))
}

#[cfg(test)]
#[path = "docs_test.rs"]
mod tests;
