//! The pictures the Snapshot panel compares — the golden, the one captured and where they differ, read from the files the test run left — side by side, one over the other, or swiped between.

use std::path::Path;
use std::sync::Arc;

use telar::preview::snapshot::SnapshotResult;
use telar::{
    Accessible, AlignItems, Clip, ClippedItem, Container, Image, ImageData, JustifyContent,
    LayoutError, LayoutItem, LayoutStyle, ObjectFit, Raster, Rect, RectStyle, Role, RwSignal,
    ShapeStyle, SizeDimension, StyledContainer, Text, box_item, children, effect, signal,
    track_layout, use_theme_tokens,
};
use telar_components::{SliderProps, slider};
use telar_devtools::{WORKBENCH_GRID, use_workbench_tokens, workbench_muted};
use tiny_skia::Pixmap;

use super::review::CompareMode;
use crate::strings::{
    self, SNAPSHOT_BASELINE, SNAPSHOT_CURRENT, SNAPSHOT_DIFF, SNAPSHOT_NO_GOLDEN, SNAPSHOT_OPACITY,
    SNAPSHOT_SWIPE_POSITION,
};

const DIVIDER_WIDTH: f32 = 2.0;
const SLIDER_WIDTH: f32 = WORKBENCH_GRID * 30.0;

/// The pictures of one snapshot that could be read.
#[derive(Clone)]
pub(super) struct Pictures {
    /// The golden as it is on disk now.
    pub(super) baseline: Option<Arc<ImageData>>,
    /// What the run captured, while it is not the golden.
    pub(super) current: Option<Arc<ImageData>>,
    pub(super) diff: Option<Arc<ImageData>>,
}

impl Pictures {
    pub(super) fn of(result: &SnapshotResult) -> Self {
        let png = result.png.as_ref();
        Self {
            baseline: png.and_then(|png| decode(&png.golden)),
            current: png.and_then(|png| png.actual.as_deref()).and_then(decode),
            diff: png.and_then(|png| png.diff.as_deref()).and_then(decode),
        }
    }

    /// Whether there is a golden and a picture captured to set against each other.
    pub(super) fn comparable(&self) -> bool {
        self.baseline.is_some() && self.current.is_some()
    }
}

fn decode(path: &Path) -> Option<Arc<ImageData>> {
    let bytes = std::fs::read(path).ok()?;
    let pixmap = Pixmap::decode_png(&bytes).ok()?;
    let (width, height) = (pixmap.width(), pixmap.height());
    Some(Arc::new(ImageData::from_premultiplied(
        pixmap.take(),
        width,
        height,
    )))
}

/// The pictures as `mode` sets them; side by side whatever `mode` says when there is no pair to set against each other.
pub(super) fn compared(
    pictures: &Pictures,
    mode: CompareMode,
    opacity: RwSignal<f32>,
    swipe: RwSignal<f32>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    match (&pictures.baseline, &pictures.current, mode) {
        (Some(baseline), Some(current), CompareMode::Overlay) => {
            overlay(baseline.clone(), current.clone(), opacity)
        }
        (Some(baseline), Some(current), CompareMode::Swipe) => {
            swiped(baseline.clone(), current.clone(), swipe)
        }
        _ => side_by_side(pictures),
    }
}

fn side_by_side(pictures: &Pictures) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let mut columns = vec![match &pictures.baseline {
        Some(baseline) => captioned(SNAPSHOT_BASELINE, area(vec![fitted(baseline.clone())?])?)?,
        None => captioned(SNAPSHOT_BASELINE, no_golden()?)?,
    }];
    if let Some(current) = &pictures.current {
        columns.push(captioned(
            SNAPSHOT_CURRENT,
            area(vec![fitted(current.clone())?])?,
        )?);
    }
    if let Some(diff) = &pictures.diff {
        columns.push(captioned(
            SNAPSHOT_DIFF,
            area(vec![fitted(diff.clone())?])?,
        )?);
    }
    let row = Container::new(fill().flex_row().gap(WORKBENCH_GRID), columns)?;
    Ok(box_item(row))
}

fn overlay(
    baseline: Arc<ImageData>,
    current: Arc<ImageData>,
    opacity: RwSignal<f32>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let over = StyledContainer::new(
        LayoutStyle::new().absolute_fill(),
        |_| RectStyle::default(),
        vec![fitted(current)?],
    )?
    .with_opacity(move || opacity.get());
    let pictures = area(vec![fitted(baseline)?, box_item(over)])?;
    with_slider(pictures, opacity, SNAPSHOT_OPACITY)
}

/// The golden from the left edge of the picture up to the line, and the picture captured past it.
fn swiped(
    baseline: Arc<ImageData>,
    current: Arc<ImageData>,
    position: RwSignal<f32>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let size = signal(Rect::default());
    let intrinsic = (current.width as f32, current.height as f32);
    let line = move || {
        let shown = contained(intrinsic, size.get());
        shown.x + shown.width * position.get().clamp(0.0, 1.0)
    };
    let whole = move || {
        let Rect { width, height, .. } = size.get();
        LayoutStyle::new()
            .absolute()
            .inset_top(0.0)
            .inset_start(0.0)
            .width(width)
            .height(height)
    };
    let held = Container::new(whole(), vec![fitted(baseline)?])?.styled_by(whole);
    let up_to_line = move || {
        LayoutStyle::new()
            .absolute()
            .inset_top(0.0)
            .inset_bottom(0.0)
            .inset_start(0.0)
            .width(line())
    };
    let cut = Container::new(up_to_line(), children![held])?.styled_by(up_to_line);
    let cut = ClippedItem::new(box_item(cut), Clip::both().paint_only());
    let at_line = move || {
        LayoutStyle::new()
            .absolute()
            .inset_top(0.0)
            .inset_bottom(0.0)
            .inset_start(line() - DIVIDER_WIDTH / 2.0)
            .width(DIVIDER_WIDTH)
    };
    let divider = StyledContainer::new(
        at_line(),
        |_| RectStyle::default().with_fill(use_theme_tokens().primary()),
        Vec::new(),
    )?
    .styled_by(at_line);
    let pictures = area(vec![fitted(current)?, box_item(cut), box_item(divider)])?;
    if let Some(tracked) = track_layout(pictures.layout_node()) {
        effect(move || {
            size.set_if_changed(tracked.get());
        });
    }
    with_slider(pictures, position, SNAPSHOT_SWIPE_POSITION)
}

/// Where a picture `intrinsic` pixels large lands when contained in `area`, centred.
fn contained((width, height): (f32, f32), area: Rect) -> Rect {
    if width <= 0.0 || height <= 0.0 {
        return Rect::new(0.0, 0.0, area.width, area.height);
    }
    let scale = (area.width / width).min(area.height / height);
    let (shown_width, shown_height) = (width * scale, height * scale);
    Rect::new(
        (area.width - shown_width) / 2.0,
        (area.height - shown_height) / 2.0,
        shown_width,
        shown_height,
    )
}

fn with_slider(
    pictures: Box<dyn LayoutItem>,
    value: RwSignal<f32>,
    label: &'static str,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let slider = slider(
        SliderProps::props()
            .value(value)
            .min(0.0)
            .max(1.0)
            .width(SLIDER_WIDTH)
            .label(strings::reactive(label))
            .build(),
    )?;
    let column = Container::new(fill().gap(WORKBENCH_GRID), vec![pictures, slider])?;
    Ok(box_item(column))
}

/// A picture laid over the whole of the box it is in, at its own proportions.
fn fitted(data: Arc<ImageData>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let image = Image::new(
        LayoutStyle::new()
            .absolute_fill()
            .width(SizeDimension::Percent(1.0))
            .height(SizeDimension::Percent(1.0)),
        move || Arc::clone(&data),
        || Raster::Smooth,
        || ObjectFit::Contain,
    )?;
    Ok(box_item(image))
}

/// The box pictures are laid over each other in, over the canvas backdrop.
fn area(layers: Vec<Box<dyn LayoutItem>>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let area = StyledContainer::new(
        fill(),
        |_| RectStyle::default().with_fill(use_workbench_tokens().canvas_backdrop),
        layers,
    )?;
    Ok(box_item(area))
}

fn captioned(
    key: &'static str,
    picture: Box<dyn LayoutItem>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let caption = Text::declaring(
        move || strings::text(key),
        LayoutStyle::new().flex_shrink(0.0),
        |text| workbench_muted(text.with_font_weight(600)),
    )?;
    let column = Container::new(
        fill().gap(WORKBENCH_GRID / 2.0),
        vec![box_item(caption), picture],
    )?
    .role(Role::Drawing)
    .a11y_label(move || strings::text(key));
    Ok(box_item(column))
}

fn no_golden() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let text = Text::declaring(
        || strings::text(SNAPSHOT_NO_GOLDEN),
        LayoutStyle::new(),
        workbench_muted,
    )?;
    let centred = StyledContainer::new(
        fill()
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER),
        |_| RectStyle::default().with_fill(use_workbench_tokens().canvas_backdrop),
        children![text],
    )?;
    Ok(box_item(centred))
}

fn fill() -> LayoutStyle {
    LayoutStyle::column()
        .flex_grow(1.0)
        .flex_basis(0.0)
        .min_width(0.0)
        .min_height(0.0)
}
