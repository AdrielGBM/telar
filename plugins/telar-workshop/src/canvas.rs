//! The canvas: the selected preview, mounted on a surface of its own so its overlays, focus, size and environment stay inside it, under a header naming it, or with no header where the workshop shows no chrome.
//!
//! Each mount is a new [`telar::SurfaceCanvas`] shown through a [`telar::SurfaceFrame`], with the preview built inside an error boundary by [`mount_preview`]: a failed build or a panic shows a card in the canvas, and the workshop around it keeps running. An arg the preview read at build builds it again inside the same canvas; [`WorkshopState::remount`] replaces the canvas itself. The preview's callbacks log their calls in the workshop's one action log, which each mount empties.

pub(crate) mod env;
mod frame;
mod guides;
pub(crate) mod states;

use std::rc::Rc;

use telar::preview::host::mount_preview;
use telar::preview::{PreviewCtx, PreviewEntry};
use telar::{
    AlignItems, Children, LayoutError, LayoutItem, LayoutStyle, Reactive, ReactiveList, RectStyle,
    ShapeStyle, SizeDimension, Slots, StyledContainer, Text, box_item,
};
use telar_components::{ButtonProps, IconButtonProps, ToolbarProps, button, icon_button, toolbar};
use telar_devtools::{WORKBENCH_GRID, use_workbench_tokens, workbench_fill};
use telar_icons::icon;

use crate::canvas_toolbar;
use crate::state::WorkshopState;
use crate::strings::{self, CANVAS_TOOLS, COPY_LINK, REMOUNT};

pub(crate) const HEADER_HEIGHT: f32 = WORKBENCH_GRID * 5.0;

/// What fills the canvas pane the shell sizes: the canvas under its header.
pub(crate) fn canvas(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    canvas_with(state, Header::Shown)
}

/// The canvas with no header over it, for the workshop without its chrome: an embedding or an editor panel shows the preview and nothing that acts on it.
pub(crate) fn bare(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    canvas_with(state, Header::Hidden)
}

#[derive(Clone, Copy)]
enum Header {
    Shown,
    Hidden,
}

fn canvas_with(state: &WorkshopState, header: Header) -> Result<Box<dyn LayoutItem>, LayoutError> {
    if state.entries().is_empty() {
        return states::empty();
    }
    let selected = state.clone();
    let mounting = state.clone();
    let canvas = ReactiveList::with_style(
        fill(),
        move || {
            let remounts = selected.remounts();
            selected
                .selected()
                .map(|entry| (entry, remounts))
                .into_iter()
                .collect::<Vec<_>>()
        },
        |(entry, remounts): &(PreviewEntry, u64)| (entry.id, *remounts),
        move |(entry, _)| mount(&mounting, entry, header),
    )?;
    Ok(box_item(canvas))
}

fn mount(
    state: &WorkshopState,
    entry: PreviewEntry,
    header: Header,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let actions = state.actions();
    actions.clear();
    let ctx = PreviewCtx::for_entry_with(&entry, state.args(&entry)).with_actions(actions);
    let globals = ctx.globals();
    let failing = state.clone();
    let surface = env::canvas(state, &entry, globals, move || {
        mount_preview(
            &entry,
            ctx,
            move |failure| states::failure(&failing, &entry, failure),
            None,
        )
    })?;
    let stage = frame::stage(state, entry.layout, globals, surface)?;
    let items = match header {
        Header::Shown => vec![self::header(state, &entry, stage.size)?, stage.item],
        Header::Hidden => vec![stage.item],
    };
    let column = StyledContainer::new(fill(), |_| RectStyle::default(), items)?;
    Ok(box_item(column))
}

fn header(
    state: &WorkshopState,
    entry: &PreviewEntry,
    size: Option<frame::SizeLabel>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let title = format!("{} / {}", entry.title, entry.name);
    let title = Text::declaring(
        move || title.clone(),
        LayoutStyle::new().flex_grow(1.0).min_width(0.0),
        |text| text.with_font_weight(600),
    )?;
    let mut items = vec![box_item(title)];
    let framed = size.is_some();
    if let Some(size) = size {
        items.push(canvas_toolbar::frame_settings(state, entry)?);
        items.push(size.build(state)?);
    }
    items.push(canvas_tools(state, entry, framed)?);
    let header = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .flex_shrink(0.0)
            .height(HEADER_HEIGHT)
            .padding_horizontal(WORKBENCH_GRID * 1.5)
            .gap(WORKBENCH_GRID),
        |_| {
            let tokens = use_workbench_tokens();
            RectStyle::default().with_fill(tokens.panel_background)
        },
        items,
    )?;
    Ok(box_item(header))
}

/// The canvas header's toolbar: what acts on the mounted preview, the grid over it, and on a framed one, its rotation and the rulers along it.
fn canvas_tools(
    state: &WorkshopState,
    entry: &PreviewEntry,
    framed: bool,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let state = state.clone();
    let asked = entry.env.viewport;
    let tools = Children::new(move || {
        let mut slots = Slots::new();
        if framed {
            slots.push(None, canvas_toolbar::rotate(&state, asked)?);
        }
        slots.push(None, canvas_toolbar::grid(&state)?);
        if framed {
            slots.push(None, canvas_toolbar::rulers(&state)?);
        }
        let linking = state.clone();
        let state = state.clone();
        let remount = icon_button(
            IconButtonProps::props()
                .icon(icon!("lucide:rotate-cw"))
                .label(Reactive::of(|| strings::text(REMOUNT)))
                .on_press(Rc::new(move || state.remount()))
                .build(),
            Children::default(),
        )?;
        slots.push(None, remount);
        let copy_link = icon_button(
            IconButtonProps::props()
                .icon(icon!("lucide:link"))
                .label(Reactive::of(|| strings::text(COPY_LINK)))
                .on_press(Rc::new(move || linking.copy_link()))
                .build(),
            Children::default(),
        )?;
        slots.push(None, copy_link);
        Ok(slots)
    });
    toolbar(
        ToolbarProps::props()
            .label(Reactive::of(|| strings::text(CANVAS_TOOLS)))
            .build(),
        tools,
    )
}

fn remount_button(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let state = state.clone();
    button(
        ButtonProps::props()
            .label(Reactive::of(|| strings::text(REMOUNT)))
            .ghost(true)
            .on_press(Rc::new(move || state.remount()))
            .build(),
        Children::default(),
    )
}

fn fill() -> LayoutStyle {
    workbench_fill().width(SizeDimension::Percent(1.0))
}

#[cfg(test)]
#[path = "canvas_test.rs"]
mod tests;
