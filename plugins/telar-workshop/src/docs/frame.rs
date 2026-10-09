//! A preview as the Docs page shows it: on a canvas of its own, as wide as the page and as tall as what it draws, in the environment the toolbar sets, and mounted afresh on a remount.

use telar::preview::host::mount_preview;
use telar::preview::{Layout, PreviewCtx, PreviewEntry};
use telar::{
    LayoutError, LayoutItem, LayoutStyle, Lazy, ReactiveList, RectStyle, SizeDimension,
    StyledContainer, SurfaceFrame, ViewRange, box_item, effect, scroll_progress, signal,
};
use telar_devtools::{WORKBENCH_GRID, workbench_card};

use crate::canvas::{env, states};
use crate::state::WorkshopState;

/// The height a canvas has before what it draws is measured, and the least it is ever given.
const MIN_HEIGHT: f32 = WORKBENCH_GRID * 12.0;
/// A fullscreen preview fills whatever it is given, so it has no height of its own to measure.
const FULLSCREEN_HEIGHT: f32 = WORKBENCH_GRID * 45.0;
const BORDER: f32 = 1.0;

/// `entry` mounted the first time its place on the page scrolls into view, and kept from then on.
pub(super) fn lazy(
    state: &WorkshopState,
    entry: PreviewEntry,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let seen = signal(false);
    let building = state.clone();
    let deferred = Lazy::new(
        LayoutStyle::new().flex_column(),
        move || seen.get(),
        move || Ok(vec![eager(&building, entry)?]),
    )?;
    let slot = StyledContainer::new(
        LayoutStyle::new().flex_column().min_height(MIN_HEIGHT),
        |_| RectStyle::default(),
        vec![box_item(deferred)],
    )?;
    let progress = scroll_progress(slot.layout_node(), ViewRange::Cover);
    effect(move || {
        if progress.get() > 0.0 {
            seen.set_if_changed(true);
        }
    });
    Ok(box_item(slot))
}

/// `entry` mounted as the page is built.
pub(super) fn eager(
    state: &WorkshopState,
    entry: PreviewEntry,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let counted = state.clone();
    let mounting = state.clone();
    let mounts = ReactiveList::with_style(
        LayoutStyle::new().flex_column(),
        move || vec![counted.remounts()],
        |remounts: &u64| *remounts,
        move |_| mount(&mounting, entry),
    )?;
    Ok(box_item(mounts))
}

fn mount(state: &WorkshopState, entry: PreviewEntry) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let ctx = PreviewCtx::for_entry_with(&entry, state.args(&entry)).with_actions(state.actions());
    let globals = ctx.globals();
    let drawn = signal(MIN_HEIGHT);
    let failing = state.clone();
    let surface = env::canvas_unframed(state, &entry.env, globals, move || {
        mount_preview(
            &entry,
            ctx,
            move |failure| states::failure(&failing, &entry, failure),
            Some(drawn),
        )
    })?;
    let frame = SurfaceFrame::filling(
        surface,
        LayoutStyle::new()
            .flex_grow(1.0)
            .min_height(0.0)
            .width(SizeDimension::Percent(1.0)),
    )?;
    let fixed = (entry.layout == Layout::Fullscreen).then(|| {
        entry
            .env
            .viewport
            .map_or(FULLSCREEN_HEIGHT, |size| size.height)
    });
    let height = move || fixed.unwrap_or_else(|| drawn.get().max(MIN_HEIGHT));
    let box_style = move || {
        LayoutStyle::new()
            .flex_column()
            .flex_shrink(0.0)
            .padding_all(BORDER)
            .height(height() + 2.0 * BORDER)
    };
    let framed = StyledContainer::new(box_style(), |_| workbench_card(), vec![box_item(frame)])?
        .styled_by(box_style);
    Ok(box_item(framed))
}
