//! The canvas: the selected preview, mounted on a surface of its own so its overlays, focus, size and environment stay inside it, under a header naming it.
//!
//! Each mount is a new [`SurfaceCanvas`] shown through a [`telar::SurfaceFrame`], with the preview built inside an error boundary by [`remounting`]: a failed build or a panic shows a card in the canvas, and the workshop around it keeps running. An arg the preview read at build builds it again inside the same canvas; [`WorkshopState::remount`] replaces the canvas itself.

mod frame;
mod states;

use std::rc::Rc;

use telar::preview::host::remounting;
use telar::preview::{Globals, Layout, PreviewCtx, PreviewEntry};
use telar::{
    AlignItems, Children, Color, Container, JustifyContent, LayoutError, LayoutItem, LayoutStyle,
    Reactive, ReactiveList, RectStyle, RwSignal, ShapeStyle, Size, SizeDimension, Slots,
    StyledContainer, SurfaceCanvas, Text, box_item, effect, use_theme_tokens,
};
use telar_components::{ButtonProps, IconButtonProps, ToolbarProps, button, icon_button, toolbar};
use telar_devtools::{WORKBENCH_GRID, use_workbench_tokens};
use telar_icons::icon;

use crate::state::WorkshopState;
use crate::strings::{self, CANVAS_TOOLS, REMOUNT};

pub(crate) const HEADER_HEIGHT: f32 = WORKBENCH_GRID * 4.0;
pub(crate) const PAGE_PADDING: f32 = WORKBENCH_GRID * 2.0;

/// What fills the canvas pane the shell sizes.
pub(crate) fn canvas(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
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
        move |(entry, _)| mount(&mounting, entry),
    )?;
    Ok(box_item(canvas))
}

fn mount(state: &WorkshopState, entry: PreviewEntry) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let globals = Globals::seeded(&entry.env);
    let ctx = PreviewCtx::from(state.args(&entry)).with_globals(globals);
    let failing = state.clone();
    let surface = Rc::new(SurfaceCanvas::new(Size::ZERO, move || {
        let preview = remounting(
            ctx,
            move |ctx| entry.build_root(ctx),
            move |failure| states::failure(&failing, &entry, failure),
        )?;
        page(entry.layout, globals.background(), preview)
    })?);
    apply_env(state, &entry, globals, &surface);
    let stage = frame::stage(state, entry.layout, globals, surface)?;
    let column = StyledContainer::new(
        fill(),
        |_| RectStyle::default(),
        vec![header(state, &entry, stage.size)?, stage.item],
    )?;
    Ok(box_item(column))
}

/// Applies `entry`'s [`telar::preview::PreviewEnv`] to its canvas, the one place a canvas takes its environment from: the direction, locale, control size and viewport through `globals`, which the toolbar will write too, and the background through the page the preview is placed on.
///
/// The viewport the handles set wins over the one the preview asks for, and a fullscreen preview takes neither, filling the pane.
///
/// The mode is not applied yet: a mode of its own needs a theme per surface, which canvases do not have, so a canvas takes the application's.
fn apply_env(
    state: &WorkshopState,
    entry: &PreviewEntry,
    globals: Globals,
    canvas: &Rc<SurfaceCanvas>,
) {
    let (chosen, asked) = (state.viewport(), entry.env.viewport);
    let framed = entry.layout != Layout::Fullscreen;
    effect(move || {
        let viewport = chosen
            .get()
            .map(|(width, height)| Size::new(width, height))
            .or(asked);
        globals.viewport().set(viewport.filter(|_| framed));
    });
    globals.bind(canvas);
}

/// The surface's whole content: the preview placed as `layout` asks, on the background the environment names or the application theme's surface.
fn page(
    layout: Layout,
    background: RwSignal<Option<Color>>,
    preview: Box<dyn LayoutItem>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let style = match layout {
        Layout::Padded => fill()
            .padding_all(PAGE_PADDING)
            .align_items(AlignItems::FLEX_START),
        Layout::Centered => fill()
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER),
        Layout::Fullscreen => fill(),
    };
    let preview = match layout {
        Layout::Fullscreen => preview,
        // The mount fills the box it is given, so a preview placed at its own size is given a box that size.
        Layout::Padded | Layout::Centered => box_item(Container::new(
            LayoutStyle::new().flex_column(),
            vec![preview],
        )?),
    };
    let page = StyledContainer::new(
        style.height(SizeDimension::Percent(1.0)),
        move |_| {
            let fill = background
                .get()
                .unwrap_or_else(|| use_theme_tokens().surface());
            RectStyle::default().with_fill(fill)
        },
        vec![preview],
    )?;
    Ok(box_item(page))
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
    if let Some(size) = size {
        items.push(size.build(state)?);
    }
    items.push(canvas_tools(state)?);
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

/// The canvas header's toolbar: what acts on the mounted preview.
fn canvas_tools(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let state = state.clone();
    let tools = Children::new(move || {
        let state = state.clone();
        let remount = icon_button(
            IconButtonProps::props()
                .icon(icon!("lucide:rotate-cw"))
                .label(Reactive::of(|| strings::text(REMOUNT)))
                .on_press(Rc::new(move || state.remount()))
                .build(),
            Children::default(),
        )?;
        let mut slots = Slots::new();
        slots.push(None, remount);
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
    LayoutStyle::new()
        .flex_column()
        .flex_grow(1.0)
        .width(SizeDimension::Percent(1.0))
        .min_width(0.0)
        .min_height(0.0)
}

#[cfg(test)]
#[path = "canvas_test.rs"]
mod tests;
