//! The Docs view: a page per component, written from what its previews carry.
//!
//! The page is the selected preview's component, as the sidebar lists it by title. It opens with the prose its previews carry and the props' own doc, shows the component's first preview with its controls and source, lists the props, and then shows every other preview over its source, each mounted the first time it scrolls into view. The previews' callbacks log their calls in the workshop's one action log, which each page empties as it opens.

mod frame;
mod props;

use std::rc::Rc;

use telar::preview::PreviewEntry;
use telar::{
    Accessible, AlignItems, Children, JustifyContent, LayoutError, LayoutItem, LayoutScrollArea,
    LayoutStyle, Reactive, ReactiveList, RectStyle, Role, StyledContainer, Text, box_item,
};
use telar_components::{ButtonProps, CodeViewProps, button, code_view};
use telar_devtools::{WORKBENCH_GRID, workbench_fill, workbench_muted};

use crate::panels::arg_table;
use crate::state::{ViewMode, WorkshopState};
use crate::strings::{self, NO_PREVIEWS, OPEN_IN_CANVAS, PREVIEWS, PROPS, SOURCE_OF};

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
                .map(|entry| entry.title)
                .into_iter()
                .collect()
        },
        |title: &&'static str| *title,
        move |title| page(&building, title),
    )?;
    Ok(box_item(docs))
}

/// The page of the component listed under `title`.
fn page(state: &WorkshopState, title: &'static str) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let entries: Vec<PreviewEntry> = state
        .entries()
        .iter()
        .filter(|entry| entry.title == title)
        .copied()
        .collect();
    let Some((primary, others)) = entries.split_first() else {
        return message(NO_PREVIEWS);
    };
    state.actions().clear();
    let schema = entries
        .iter()
        .find_map(|entry| entry.props)
        .map(|props| props());
    let mut items = heading(title)?;
    if let Some(docs) = entries
        .iter()
        .map(|entry| entry.docs)
        .find(|docs| !docs.is_empty())
    {
        items.extend(prose(docs)?);
    }
    if let Some(doc) = schema.map(|schema| schema.doc) {
        items.extend(prose(doc)?);
    }
    items.push(frame::eager(state, *primary)?);
    items.push(arg_table(state.args(primary))?);
    items.extend(source(primary)?);
    if let Some(schema) = schema.filter(|schema| !schema.fields.is_empty()) {
        items.push(section_heading(PROPS)?);
        items.push(props::table(schema)?);
    }
    if !others.is_empty() {
        items.push(section_heading(PREVIEWS)?);
        for entry in others {
            items.push(preview(state, *entry)?);
        }
    }
    let column = StyledContainer::new(
        LayoutStyle::new()
            .flex_column()
            .gap(WORKBENCH_GRID * 2.0)
            .padding_all(PAGE_MARGIN)
            .max_width(PAGE_MAX_WIDTH),
        |_| RectStyle::default(),
        items,
    )?;
    let scroll = LayoutScrollArea::new(workbench_fill(), box_item(column))?;
    Ok(box_item(scroll))
}

/// The component's name over the group path it is listed under.
fn heading(title: &'static str) -> Result<Vec<Box<dyn LayoutItem>>, LayoutError> {
    let (group, name) = title.rsplit_once('/').unwrap_or(("", title));
    let mut items = Vec::new();
    if !group.is_empty() {
        items.push(box_item(Text::declaring(
            move || group.replace('/', " / "),
            LayoutStyle::new(),
            workbench_muted,
        )?));
    }
    let name = Text::declaring(
        move || name.to_string(),
        LayoutStyle::new(),
        |text| text.with_font_size(TITLE_SIZE).with_font_weight(600),
    )?;
    let name = StyledContainer::new(
        LayoutStyle::new().flex_column(),
        |_| RectStyle::default(),
        vec![box_item(name)],
    )?
    .role(Role::Heading(1));
    items.push(box_item(name));
    Ok(items)
}

fn section_heading(key: &'static str) -> Result<Box<dyn LayoutItem>, LayoutError> {
    titled(Role::Heading(2), SECTION_SIZE, move || strings::text(key))
}

fn titled(
    role: Role,
    size: f32,
    text: impl Fn() -> String + 'static,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let text = Text::declaring(text, LayoutStyle::new(), move |text| {
        text.with_font_size(size).with_font_weight(600)
    })?;
    let heading = StyledContainer::new(
        LayoutStyle::new().flex_column().padding_top(WORKBENCH_GRID),
        |_| RectStyle::default(),
        vec![box_item(text)],
    )?
    .role(role);
    Ok(box_item(heading))
}

/// `text` as paragraphs, one per run of lines between blank ones, each run's lines joined as one wrapped line.
fn prose(text: &'static str) -> Result<Vec<Box<dyn LayoutItem>>, LayoutError> {
    paragraphs(text)
        .into_iter()
        .map(|paragraph| {
            let paragraph =
                Text::declaring(move || paragraph.clone(), LayoutStyle::new(), |text| text)?;
            Ok(box_item(paragraph))
        })
        .collect()
}

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

/// One preview after the first: its name and a way to open it on the canvas, the preview mounted once it scrolls into view, and its source.
fn preview(state: &WorkshopState, entry: PreviewEntry) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let name = entry.name;
    let title = StyledContainer::new(
        LayoutStyle::new().flex_grow(1.0).min_width(0.0),
        |_| RectStyle::default(),
        vec![titled(Role::Heading(3), SECTION_SIZE * 0.875, move || {
            name.to_string()
        })?],
    )?;
    let opening = state.clone();
    let open = button(
        ButtonProps::props()
            .label(Reactive::of(|| strings::text(OPEN_IN_CANVAS)))
            .ghost(true)
            .on_press(Rc::new(move || {
                opening.select(entry.id);
                opening.view().set(ViewMode::Canvas);
            }))
            .build(),
        Children::default(),
    )?
    .a11y_label(move || format!("{}: {name}", strings::text(OPEN_IN_CANVAS)));
    let header = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::FLEX_END)
            .gap(WORKBENCH_GRID),
        |_| RectStyle::default(),
        vec![box_item(title), open],
    )?;
    let mut items = vec![box_item(header), frame::lazy(state, entry)?];
    items.extend(source(&entry)?);
    let section = StyledContainer::new(
        LayoutStyle::new().flex_column().gap(WORKBENCH_GRID),
        |_| RectStyle::default(),
        items,
    )?;
    Ok(box_item(section))
}

/// The preview's source as it is written, numbered from the line it starts on where that is known.
fn source(entry: &PreviewEntry) -> Result<Option<Box<dyn LayoutItem>>, LayoutError> {
    if entry.source.trim().is_empty() {
        return Ok(None);
    }
    let name = entry.name;
    let first_line = entry.source_spans.first().map(|span| span.line);
    let view = code_view(
        CodeViewProps::props()
            .code(entry.source)
            .line_numbers(first_line.is_some())
            .first_line(first_line.unwrap_or(1))
            .label(Reactive::of(move || strings::text_with(SOURCE_OF, name)))
            .build(),
        Children::default(),
    )?;
    Ok(Some(view))
}

fn message(key: &'static str) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let message = Text::declaring(
        move || strings::text(key),
        LayoutStyle::new(),
        workbench_muted,
    )?;
    let centred = StyledContainer::new(
        workbench_fill()
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER),
        |_| RectStyle::default(),
        vec![box_item(message)],
    )?
    .role(Role::Status);
    Ok(box_item(centred))
}

#[cfg(test)]
#[path = "docs_test.rs"]
mod tests;
