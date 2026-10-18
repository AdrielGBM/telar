//! The canvas: the selected preview, mounted on a surface of its own so its overlays, focus, size and environment stay inside it, under a header naming it, or with no header where the workshop shows no chrome.
//!
//! Each mount is a new [`telar::SurfaceCanvas`] shown through a [`telar::SurfaceFrame`], with the preview built inside an error boundary by [`mount_preview`]: a failed build or a panic shows a card in the canvas, and the workshop around it keeps running. An arg the preview read at build builds it again inside the same canvas; [`WorkshopState::remount`] replaces the canvas itself. The preview's callbacks log their calls in the workshop's one action log, which each mount empties, and its play drives the canvas as the interactions panel says.

telar::rsx_modules!();

pub(crate) mod env;
mod frame;
mod guides;
pub(crate) mod inspected;
pub(crate) mod states;

use std::cell::Cell;
use std::rc::Rc;

use telar::preview::host::mount_preview;
use telar::preview::{CaughtFailures, PreviewCtx, PreviewEntry};
use telar::{
    Accessible, AlignItems, Clip, ClippedItem, Container, LayoutError, LayoutItem, LayoutStyle,
    PointerButton, ReactiveList, Rect, RectStyle, Role, RwSignal, ShapeStyle, SizeDimension,
    StyledContainer, Text, TextWrap, box_item, children, effect, memo, modifiers, on_cleanup,
    pointer_position, signal, track_layout,
};
use telar_devtools::{WORKBENCH_GRID, use_workbench_tokens, workbench_fill};

use crate::more_menu::{Asked, menu_layer, more_button};
use crate::state::WorkshopState;
use crate::strings::MORE_TOOLS;
use crate::{canvas_toolbar, interactions};

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
    let args = state.args(&entry);
    args.restore();
    let ctx = PreviewCtx::for_entry_with(&entry, args).with_actions(actions);
    let globals = ctx.globals();
    let failing = state.clone();
    let failures = CaughtFailures::default();
    let (playing, caught) = (ctx.clone(), failures.clone());
    let surface = env::canvas(state, &entry, globals, move || {
        mount_preview(
            &entry,
            ctx,
            move |failure| {
                caught.record(&failure);
                states::failure(&failing, &entry, failure)
            },
            None,
        )
    })?;
    interactions::drive(state, &entry, playing, &surface, failures);
    let following = state
        .inspection()
        .follow(inspected::PreviewOnly(Rc::clone(&surface)));
    on_cleanup(move || drop(following));
    let stage = frame::stage(state, entry.layout, globals, surface)?;
    let items = match header {
        Header::Shown => {
            let tools = Rc::new(canvas_toolbar::HeaderTools::new(
                state,
                &entry,
                stage.size.is_some(),
            ));
            vec![
                self::header(state, &entry, stage.size, &tools)?,
                acting_on(state, stage.item, &tools)?,
            ]
        }
        Header::Hidden => vec![stage.item],
    };
    let column = Container::new(fill(), items)?;
    Ok(box_item(column))
}

const HEADER_PAD: f32 = WORKBENCH_GRID * 1.5;
/// The least the preview's name keeps beside the tools before they fold into one menu.
const TITLE_ROOM: f32 = WORKBENCH_GRID * 12.0;

fn header(
    state: &WorkshopState,
    entry: &PreviewEntry,
    size: Option<frame::SizeLabel>,
    tools: &Rc<canvas_toolbar::HeaderTools>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let name = format!("{} / {}", entry.title, entry.name);
    let shown = name.clone();
    let title = Text::declaring(
        move || shown.clone(),
        LayoutStyle::new().min_width(0.0),
        |text| text.with_font_weight(600).with_text_wrap(TextWrap::NoWrap),
    )?;
    let title = Container::new(
        LayoutStyle::new().flex_grow(1.0).min_width(0.0),
        children![ClippedItem::new(box_item(title), Clip::both())],
    )?
    .role(Role::Status)
    .a11y_label(move || name.clone());
    let tools = Rc::clone(tools);
    let laid = signal(None::<RwSignal<Rect>>);
    let spread = signal(0.0_f32);
    // Against the width the tools took the last time they were spread out, so folding them cannot change the answer that folded them.
    let folded = memo(move || {
        let Some(header) = laid.get() else {
            return false;
        };
        let room = header.get().width - 2.0 * HEADER_PAD - WORKBENCH_GRID - TITLE_ROOM;
        spread.get() > room
    });
    let state = state.clone();
    let controls = ReactiveList::with_style(
        LayoutStyle::row()
            .align_items(AlignItems::CENTER)
            .flex_shrink(0.0),
        move || vec![folded.get()],
        |folded: &bool| *folded,
        move |folded| match folded {
            false => spread_out(&tools, size.clone(), &state, spread),
            true => folded_up(&tools, size.clone(), &state),
        },
    )?;
    let header = StyledContainer::new(
        LayoutStyle::row()
            .align_items(AlignItems::CENTER)
            .flex_shrink(0.0)
            .height(HEADER_HEIGHT)
            .padding_horizontal(HEADER_PAD)
            .gap(WORKBENCH_GRID),
        |_| {
            let tokens = use_workbench_tokens();
            RectStyle::default().with_fill(tokens.panel_background)
        },
        vec![box_item(title), box_item(controls)],
    )?;
    laid.set(track_layout(header.layout_node()));
    Ok(box_item(header))
}

/// The canvas's size, the toolbar, then the size and zoom of a framed preview and the theme mode behind a menu. Records in `spread` how wide they stand, which never shrinks below what they need.
fn spread_out(
    tools: &Rc<canvas_toolbar::HeaderTools>,
    size: Option<frame::SizeLabel>,
    state: &WorkshopState,
    spread: RwSignal<f32>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let mut items = Vec::new();
    if let Some(size) = size {
        items.push(size.build(state)?);
    }
    items.push(tools.toolbar()?);
    items.push(more(tools, false)?);
    let row = Container::new(tools_row(), items)?;
    let laid = track_layout(row.layout_node()).expect("the tools are laid out");
    effect(move || {
        spread.set_if_changed(laid.get().width);
    });
    Ok(box_item(row))
}

/// The tools folded into one menu behind a button, beside the canvas's size.
fn folded_up(
    tools: &Rc<canvas_toolbar::HeaderTools>,
    size: Option<frame::SizeLabel>,
    state: &WorkshopState,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let mut items = Vec::new();
    if let Some(size) = size {
        items.push(size.build(state)?);
    }
    items.push(more(tools, true)?);
    Ok(box_item(Container::new(tools_row(), items)?))
}

fn tools_row() -> LayoutStyle {
    LayoutStyle::row()
        .align_items(AlignItems::CENTER)
        .flex_shrink(0.0)
        .gap(WORKBENCH_GRID)
}

/// The button a menu of the tools opens from: the size and zoom and the theme mode, and with `all` every other tool too.
fn more(
    tools: &Rc<canvas_toolbar::HeaderTools>,
    all: bool,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let tools = Rc::clone(tools);
    more_button(MORE_TOOLS, Rc::new(move || tools.menu_rows(all)))
}

/// How far the wheel turns, in px, for one step of zoom: a notch of a mouse wheel, which a trackpad reaches by many smaller ones.
const WHEEL_STEP: f32 = 60.0;

/// The stage, which takes what its preview leaves of the pointer: a secondary press opens every tool as a menu, and the wheel with Ctrl or ⌘ held zooms the canvas.
fn acting_on(
    state: &WorkshopState,
    stage: Box<dyn LayoutItem>,
    tools: &Rc<canvas_toolbar::HeaderTools>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let tools = Rc::clone(tools);
    let open = signal(None::<Asked>);
    let settings = state.canvas_settings();
    let turned = Rc::new(Cell::new(0.0_f32));
    let stage = StyledContainer::new(fill(), |_| RectStyle::default(), vec![stage])?
        .on_alt_press(move |button| {
            if button == PointerButton::Secondary {
                open.set(Some(Asked {
                    at: pointer_position().unwrap_or_default(),
                    around: None,
                }));
            }
        })
        .on_scroll(move |_, dy| {
            let held = modifiers();
            if !(held.is_ctrl || held.is_meta) {
                turned.set(0.0);
                return;
            }
            let total = turned.get() + dy;
            let steps = (total / WHEEL_STEP).trunc();
            turned.set(total - steps * WHEEL_STEP);
            for _ in 0..steps.abs() as u32 {
                settings.update(|settings| {
                    settings.zoom = match steps > 0.0 {
                        true => settings.zoom.zoomed_in(),
                        false => settings.zoom.zoomed_out(),
                    }
                });
            }
        });
    Ok(box_item(Container::new(
        fill(),
        vec![
            box_item(stage),
            menu_layer(open, Rc::new(move || tools.menu_rows(true)))?,
        ],
    )?))
}

fn fill() -> LayoutStyle {
    workbench_fill().width(SizeDimension::Percent(1.0))
}

#[cfg(test)]
#[path = "canvas_test.rs"]
mod tests;
