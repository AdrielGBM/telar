//! Hosting previews: the [`Args`] behind each canvas that a controls panel lists and sets, the mount that builds a preview again when an arg it read changes, and the hosts `cargo telar` starts. A preview's author needs none of it.

use std::collections::BTreeMap;

use reactive_core::{RwSignal, effect};

use crate::{
    AlignItems, BuildFailure, Color, Container, JustifyContent, LayoutError, LayoutItem,
    LayoutStyle, RectStyle, ShapeStyle, SizeDimension, StyledContainer, box_item, track_layout,
    use_theme_tokens,
};

use super::{Layout, PreviewCtx, PreviewEntry};

pub use super::args::{ArgError, ArgState, Args};
pub use super::mount::remounting;

#[cfg(not(target_os = "android"))]
pub use super::app::PreviewApp;
#[cfg(all(feature = "preview-headless", not(target_os = "android")))]
pub use super::app::run_preview_png;
#[cfg(not(target_os = "android"))]
pub use super::runner::{dev_entry, try_run_test};

/// The space around a [`Layout::Padded`] preview on its [`page`], in logical px.
pub const PAGE_PADDING: f32 = 16.0;

/// `entry` built against `ctx` and placed on its [`page`], as every canvas shows a preview: [`remounting`], so a build that fails shows what `on_failure` makes of it and an arg the build read builds it again, on the background `ctx`'s globals hold.
///
/// This is what goes inside a canvas, not the canvas: the host makes the surface it mounts on, in the environment it gives it. `measured` is [`page`]'s.
pub fn mount_preview(
    entry: &PreviewEntry,
    ctx: PreviewCtx,
    on_failure: impl Fn(BuildFailure) -> Result<Box<dyn LayoutItem>, LayoutError> + 'static,
    measured: Option<RwSignal<f32>>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let entry = *entry;
    let background = ctx.globals().background();
    let preview = remounting(ctx, move |ctx| entry.build_root(ctx), on_failure)?;
    page(entry.layout, background, preview, measured)
}

/// A canvas's whole content: `preview` placed as `layout` asks, on `background`, or on the theme's surface while that is `None`.
///
/// The background is painted, not left to the window, so what is behind the preview is part of its frame: a screenshot shows it, and [`a11y::check`](super::a11y::check) measures text against it.
///
/// `measured`, when given, is kept at the height the page needs to show the preview whole — its own height with [`PAGE_PADDING`] above and below — for a host that sizes the canvas to what it draws, as a docs page does. A [`Layout::Fullscreen`] preview fills what it is given and has no height of its own, so it leaves `measured` as it is.
pub fn page(
    layout: Layout,
    background: RwSignal<Option<Color>>,
    preview: Box<dyn LayoutItem>,
    measured: Option<RwSignal<f32>>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let filling = LayoutStyle::new()
        .flex_column()
        .flex_grow(1.0)
        .width(SizeDimension::Percent(1.0))
        .height(SizeDimension::Percent(1.0))
        .min_width(0.0)
        .min_height(0.0);
    let style = match layout {
        Layout::Padded => filling
            .padding_all(PAGE_PADDING)
            .align_items(AlignItems::FLEX_START),
        Layout::Centered => filling
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER),
        Layout::Fullscreen => filling,
    };
    let preview = match layout {
        Layout::Fullscreen => preview,
        // The mount fills the box it is given, so a preview placed at its own size is given a box that size.
        Layout::Padded | Layout::Centered => {
            let placed = Container::new(LayoutStyle::new().flex_column(), vec![preview])?;
            if let Some(measured) = measured {
                follow_height(&placed, measured);
            }
            box_item(placed)
        }
    };
    let page = StyledContainer::new(
        style,
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

fn follow_height(placed: &Container, measured: RwSignal<f32>) {
    let Some(rect) = track_layout(placed.layout_node()) else {
        return;
    };
    effect(move || {
        let height = rect.get().height + 2.0 * PAGE_PADDING;
        if measured.peek() != height {
            measured.set(height);
        }
    });
}

pub use telar_project::protocol::{PREVIEW_COMPONENT_VAR, PREVIEW_ID_VAR, WORKSPACE_DIR_VAR};

/// The workspace [`WORKSPACE_DIR_VAR`] names, or `None` for a preview build started some other way.
pub fn workspace_dir() -> Option<std::path::PathBuf> {
    std::env::var_os(WORKSPACE_DIR_VAR)
        .filter(|dir| !dir.is_empty())
        .map(std::path::PathBuf::from)
}

/// What `cargo telar preview` asked a host to open: a preview by id, a component by name, or neither.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PreviewRequest {
    pub id: Option<String>,
    pub component: Option<String>,
}

impl PreviewRequest {
    /// The request `id` and `component` make, each `None` when absent or blank.
    pub fn new(id: Option<&str>, component: Option<&str>) -> Self {
        let named = |value: Option<&str>| {
            value
                .filter(|value| !value.trim().is_empty())
                .map(str::to_owned)
        };
        Self {
            id: named(id),
            component: named(component),
        }
    }

    /// Whether `entry` belongs to the requested component, by its `component` or by the last segment of its title. Any entry matches when no component was requested.
    pub fn matches_component(&self, entry: &PreviewEntry) -> bool {
        self.component.as_deref().is_none_or(|wanted| {
            entry.component == wanted || entry.title.rsplit('/').next() == Some(wanted)
        })
    }

    /// The preview to open first among `entries`: the one `id` names, otherwise the first of the requested component. `None` when neither is among them.
    pub fn resolve<'a>(&self, entries: &'a [PreviewEntry]) -> Option<&'a PreviewEntry> {
        let by_id = self
            .id
            .as_deref()
            .and_then(|id| entries.iter().find(|entry| entry.id == id));
        by_id.or_else(|| {
            self.component
                .as_ref()
                .and_then(|_| entries.iter().find(|entry| self.matches_component(entry)))
        })
    }
}

/// The request in `TELAR_PREVIEW_ID` and `TELAR_PREVIEW_COMPONENT`.
///
/// A shell reads it once, as it starts: a selection it restored across a hot reload wins over it.
pub fn requested_preview() -> PreviewRequest {
    PreviewRequest::new(
        std::env::var(PREVIEW_ID_VAR).ok().as_deref(),
        std::env::var(PREVIEW_COMPONENT_VAR).ok().as_deref(),
    )
}

/// What is wrong with `entries` when ids repeat, one line per repeated id naming where each of its previews is written, or `None` when every id is its own. An id names a deep link and a snapshot file, so two previews under one would overwrite each other.
pub fn duplicate_ids(entries: &[PreviewEntry]) -> Option<String> {
    let mut by_id: BTreeMap<&str, Vec<&PreviewEntry>> = BTreeMap::new();
    for entry in entries {
        by_id.entry(entry.id).or_default().push(entry);
    }
    let lines: Vec<String> = by_id
        .into_iter()
        .filter(|(_, shared)| shared.len() > 1)
        .map(|(id, shared)| {
            let places: Vec<String> = shared.iter().map(|entry| place(entry)).collect();
            format!(
                "preview id `{id}` names {} previews ({}); rename all but one",
                shared.len(),
                places.join(", ")
            )
        })
        .collect();
    (!lines.is_empty()).then(|| lines.join("\n"))
}

/// Prints a `FAIL` line for each id `entries` repeat, the way a run that renders or tests every preview reports one that failed, and answers how many it printed.
#[cfg(not(target_os = "android"))]
pub(crate) fn fail_duplicate_ids(entries: &[PreviewEntry]) -> usize {
    let Some(duplicates) = duplicate_ids(entries) else {
        return 0;
    };
    duplicates
        .lines()
        .inspect(|line| println!("  FAIL  {line}"))
        .count()
}

/// `id` as a file name: letters, digits and `-_.=+` kept, anything else made `-`, so a cell id such as `button--primary--mode=dark` is its own name. What a snapshot and a rendered picture of a preview or a matrix cell are named by.
pub fn file_stem(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '=' | '+') {
                c
            } else {
                '-'
            }
        })
        .collect()
}

fn place(entry: &PreviewEntry) -> String {
    match entry.file {
        "" => format!("\"{}\"", entry.name),
        file => format!("{file}:{}", entry.line),
    }
}

#[cfg(test)]
#[path = "host_test.rs"]
mod tests;
