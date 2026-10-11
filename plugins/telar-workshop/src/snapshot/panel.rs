//! The panel itself: the snapshot of the selected preview, or of the focused matrix cell, against its golden as the last test run found it, and what acts on it.

use std::rc::Rc;

use telar::preview::snapshot::{FileStatus, SnapshotResult, SnapshotStatus};
use telar::{
    AlignItems, Children, Color, JustifyContent, LayoutError, LayoutItem, LayoutScrollArea,
    LayoutStyle, Reactive, ReactiveList, RectStyle, Role, StyledContainer, Text, box_item,
    use_theme_tokens,
};
use telar_components::{
    BadgeProps, ButtonProps, CodeSpan, CodeViewProps, Tab, TokenKind, badge, button, code_view,
    tab_list,
};
use telar_devtools::{WORKBENCH_GRID, workbench_fill, workbench_mono, workbench_muted};

use super::pictures::{Pictures, compared};
use super::review::{CompareMode, Loaded, SnapshotReview};
use super::subject;
use crate::state::WorkshopState;
use crate::strings::{
    self, NOTHING_SELECTED, SNAPSHOT_ACCEPT, SNAPSHOT_ACCEPT_FAILED, SNAPSHOT_COMPARE,
    SNAPSHOT_DRAW_CHANGED, SNAPSHOT_DRAW_DIFF, SNAPSHOT_DRIFT, SNAPSHOT_FAILED, SNAPSHOT_MATCHES,
    SNAPSHOT_NO_GOLDEN, SNAPSHOT_NO_REPORT, SNAPSHOT_NO_REPORT_HINT, SNAPSHOT_NOT_TAKEN,
    SNAPSHOT_OVERLAY, SNAPSHOT_PIXELS, SNAPSHOT_RELOAD, SNAPSHOT_RESIZED, SNAPSHOT_SIDE_BY_SIDE,
    SNAPSHOT_SWIPE, SNAPSHOT_UNREADABLE, SNAPSHOT_WRITTEN,
};

const MESSAGE_MAX_WIDTH: f32 = WORKBENCH_GRID * 90.0;

/// What the panel shows for the snapshot under review.
#[derive(Clone, Debug)]
enum Shown {
    NothingSelected,
    NoReport,
    Unreadable(String),
    NotTaken,
    NotCaptured(String),
    Found(Box<SnapshotResult>),
}

fn shown(state: &WorkshopState) -> Shown {
    let Some(id) = subject(state) else {
        return Shown::NothingSelected;
    };
    let review = state.snapshots();
    match review.loaded() {
        Loaded::Absent => Shown::NoReport,
        Loaded::Unreadable(error) => Shown::Unreadable(error),
        Loaded::Read(_) => match review.result(&id) {
            None => Shown::NotTaken,
            Some(result) if result.status == SnapshotStatus::Failed => {
                Shown::NotCaptured(result.error.unwrap_or_default())
            }
            Some(result) => Shown::Found(Box::new(result)),
        },
    }
}

/// What fills the panel while its tab is the one shown.
pub(crate) fn panel(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let watching = state.clone();
    let review = state.snapshots().clone();
    let body = ReactiveList::with_style(
        workbench_fill(),
        move || vec![shown(&watching)],
        |shown: &Shown| format!("{shown:?}"),
        move |shown| match shown {
            Shown::NothingSelected => message(NOTHING_SELECTED, None, None),
            Shown::NoReport => message(SNAPSHOT_NO_REPORT, Some(SNAPSHOT_NO_REPORT_HINT), None),
            Shown::Unreadable(error) => message(SNAPSHOT_UNREADABLE, None, Some(error)),
            Shown::NotTaken => message(SNAPSHOT_NOT_TAKEN, None, None),
            Shown::NotCaptured(error) => message(SNAPSHOT_FAILED, None, Some(error)),
            Shown::Found(result) => reviewed(&review, *result),
        },
    )?;
    Ok(box_item(body))
}

/// What acts on the report, for the end of the strip while this panel is shown: reading it again after a test run.
pub(crate) fn tools(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let review = state.snapshots().clone();
    button(
        ButtonProps::props()
            .label(strings::reactive(SNAPSHOT_RELOAD))
            .ghost(true)
            .on_press(Rc::new(move || review.reload()))
            .build(),
        Children::default(),
    )
}

/// How many snapshots of the selected preview the last test run failed, in red on the panel's tab; nothing when none did.
pub(crate) fn tab_badge(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let watching = state.clone();
    let count = ReactiveList::with_style(
        LayoutStyle::new().flex_row(),
        move || {
            let drifted = watching
                .selected()
                .map_or(0, |entry| watching.snapshots().drifted(entry.id));
            (drifted > 0).then_some(drifted).into_iter().collect()
        },
        |drifted: &usize| *drifted,
        |drifted| {
            badge(
                BadgeProps::props()
                    .label(Reactive::of(move || drifted.to_string()))
                    .color(Reactive::of(|| use_theme_tokens().error()))
                    .build(),
                Children::default(),
            )
        },
    )?;
    Ok(box_item(count))
}

/// "Drifted" in red after a preview's row in the sidebar while the last test run failed one of its snapshots.
pub(crate) fn drift_badge(
    review: &SnapshotReview,
    preview: &str,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let review = review.clone();
    let preview = preview.to_string();
    let shown = ReactiveList::with_style(
        LayoutStyle::new().flex_row().flex_shrink(0.0),
        move || match review.drifted(&preview) {
            0 => Vec::new(),
            _ => vec![()],
        },
        |_: &()| (),
        |()| {
            badge(
                BadgeProps::props()
                    .label(strings::reactive(SNAPSHOT_DRIFT))
                    .color(Reactive::of(|| use_theme_tokens().error()))
                    .build(),
                Children::default(),
            )
        },
    )?;
    Ok(box_item(shown))
}

fn reviewed(
    review: &SnapshotReview,
    result: SnapshotResult,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let pictures = Pictures::of(&result);
    let mut header = vec![title(&result.id)?, summary(&result)?, spacer()?];
    if pictures.comparable() {
        header.push(mode_switch(review)?);
    }
    if result.acceptable() {
        header.push(accept(review, &result.id)?);
    }
    let header = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .flex_wrap()
            .flex_shrink(0.0)
            .align_items(AlignItems::CENTER)
            .gap(WORKBENCH_GRID * 1.5),
        |_| RectStyle::default(),
        header,
    )?;
    let (mode, opacity, swipe) = (review.mode(), review.opacity(), review.swipe());
    let shown = ReactiveList::with_style(
        fill(),
        move || vec![mode.get()],
        |mode: &CompareMode| *mode,
        move |mode| compared(&pictures, mode, opacity, swipe),
    )?;
    let mut content = vec![box_item(shown)];
    if let Some(diff) = draw_diff(&result)? {
        content.push(diff);
    }
    let content = StyledContainer::new(
        fill().flex_row().gap(WORKBENCH_GRID * 1.5),
        |_| RectStyle::default(),
        content,
    )?;
    let column = StyledContainer::new(
        workbench_fill()
            .gap(WORKBENCH_GRID)
            .padding_all(WORKBENCH_GRID * 1.5),
        |_| RectStyle::default(),
        vec![box_item(header), failure(review)?, box_item(content)],
    )?;
    Ok(box_item(column))
}

fn title(id: &str) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let id = id.to_string();
    let title = Text::declaring(
        move || id.clone(),
        LayoutStyle::new().min_width(0.0),
        |text| workbench_mono(text).with_font_weight(600),
    )?;
    Ok(box_item(title))
}

fn summary(result: &SnapshotResult) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let result = result.clone();
    let drifted = result.status.is_failure();
    let summary = Text::declaring(
        move || found(&result),
        LayoutStyle::new().min_width(0.0),
        move |text| match drifted {
            true => text.with_color(use_theme_tokens().error()),
            false => workbench_muted(text),
        },
    )?;
    let status = StyledContainer::new(
        LayoutStyle::new().flex_row().min_width(0.0),
        |_| RectStyle::default(),
        vec![box_item(summary)],
    )?
    .role(Role::Status);
    Ok(box_item(status))
}

/// What the run found of the snapshot, in words.
fn found(result: &SnapshotResult) -> String {
    match result.status {
        SnapshotStatus::Matched => strings::text(SNAPSHOT_MATCHES),
        SnapshotStatus::Updated => strings::text(SNAPSHOT_WRITTEN),
        SnapshotStatus::Missing => strings::text(SNAPSHOT_NO_GOLDEN),
        SnapshotStatus::Failed => {
            strings::text_with(SNAPSHOT_FAILED, result.error.as_deref().unwrap_or_default())
        }
        SnapshotStatus::Changed => {
            let mut parts = Vec::new();
            if let Some(png) = result
                .png
                .as_ref()
                .filter(|png| png.status == FileStatus::Changed)
            {
                parts.push(match png.golden_size {
                    Some(golden) if golden != png.size => strings::text_with_values(
                        SNAPSHOT_RESIZED,
                        &[
                            ("size", &format!("{}×{}", png.size[0], png.size[1])),
                            ("golden", &format!("{}×{}", golden[0], golden[1])),
                        ],
                    ),
                    _ => strings::text_with_values(
                        SNAPSHOT_PIXELS,
                        &[
                            ("count", &png.differing_pixels.to_string()),
                            ("delta", &png.max_channel_delta.to_string()),
                        ],
                    ),
                });
            }
            if result
                .draw
                .as_ref()
                .is_some_and(|draw| draw.status == FileStatus::Changed)
            {
                parts.push(strings::text(SNAPSHOT_DRAW_CHANGED));
            }
            parts.join(" · ")
        }
    }
}

fn mode_switch(review: &SnapshotReview) -> Result<Box<dyn LayoutItem>, LayoutError> {
    tab_list(
        review.mode(),
        vec![
            Tab::new(
                CompareMode::SideBySide,
                strings::reactive(SNAPSHOT_SIDE_BY_SIDE),
            ),
            Tab::new(CompareMode::Overlay, strings::reactive(SNAPSHOT_OVERLAY)),
            Tab::new(CompareMode::Swipe, strings::reactive(SNAPSHOT_SWIPE)),
        ],
        Color::TRANSPARENT.into(),
        Some(strings::reactive(SNAPSHOT_COMPARE)),
    )
}

/// Makes the picture and the draw text captured the goldens, as `cargo telar test --update-snapshots` would.
fn accept(review: &SnapshotReview, id: &str) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let review = review.clone();
    let id = id.to_string();
    button(
        ButtonProps::props()
            .label(strings::reactive(SNAPSHOT_ACCEPT))
            .on_press(Rc::new(move || review.accept(&id)))
            .build(),
        Children::default(),
    )
}

/// Why the last accept failed, while it stands.
fn failure(review: &SnapshotReview) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let review = review.clone();
    let line = ReactiveList::with_style(
        LayoutStyle::new().flex_row().flex_shrink(0.0),
        move || review.failure().into_iter().collect(),
        |error: &String| error.clone(),
        |error| {
            let text = Text::declaring(
                move || strings::text_with(SNAPSHOT_ACCEPT_FAILED, &error),
                LayoutStyle::new().min_width(0.0),
                |text| text.with_color(use_theme_tokens().error()),
            )?;
            let status = StyledContainer::new(
                LayoutStyle::new().flex_row().min_width(0.0),
                |_| RectStyle::default(),
                vec![box_item(text)],
            )?
            .role(Role::Status);
            Ok(box_item(status))
        },
    )?;
    Ok(box_item(line))
}

/// The line diff of the draw text against its golden, where the run wrote one.
fn draw_diff(result: &SnapshotResult) -> Result<Option<Box<dyn LayoutItem>>, LayoutError> {
    let Some(diff) = result
        .draw
        .as_ref()
        .and_then(|draw| draw.diff.as_deref())
        .and_then(|path| std::fs::read_to_string(path).ok())
    else {
        return Ok(None);
    };
    let spans = diff_spans(&diff);
    let view = code_view(
        CodeViewProps::props()
            .code(diff)
            .spans(spans)
            .line_numbers(false)
            .label(strings::reactive(SNAPSHOT_DRAW_DIFF))
            .build(),
        Children::default(),
    )?;
    let caption = Text::declaring(
        || strings::text(SNAPSHOT_DRAW_DIFF),
        LayoutStyle::new().flex_shrink(0.0),
        |text| workbench_muted(text.with_font_weight(600)),
    )?;
    let scroll = LayoutScrollArea::new(fill(), view)?;
    let column = StyledContainer::new(
        fill().gap(WORKBENCH_GRID / 2.0),
        |_| RectStyle::default(),
        vec![box_item(caption), box_item(scroll)],
    )?;
    Ok(Some(box_item(column)))
}

/// Each removed line of a unified diff as an error, each added one as a string and each hunk header as a comment.
pub(super) fn diff_spans(diff: &str) -> Vec<CodeSpan> {
    let mut spans = Vec::new();
    let mut start = 0;
    for line in diff.split_inclusive('\n') {
        let kind = match line.as_bytes() {
            [b'-', b'-', b'-', ..] | [b'+', b'+', b'+', ..] => None,
            [b'@', b'@', ..] => Some(TokenKind::Comment),
            [b'-', ..] => Some(TokenKind::Error),
            [b'+', ..] => Some(TokenKind::String),
            _ => None,
        };
        let end = start + line.trim_end_matches('\n').len();
        if let Some(kind) = kind {
            spans.push(CodeSpan::new(start..end, kind));
        }
        start += line.len();
    }
    spans
}

fn message(
    key: &'static str,
    hint: Option<&'static str>,
    detail: Option<String>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let mut items = vec![box_item(Text::declaring(
        move || match &detail {
            Some(detail) => strings::text_with(key, detail),
            None => strings::text(key),
        },
        LayoutStyle::new().max_width(MESSAGE_MAX_WIDTH),
        workbench_muted,
    )?)];
    if let Some(hint) = hint {
        items.push(box_item(Text::declaring(
            move || strings::text(hint),
            LayoutStyle::new().max_width(MESSAGE_MAX_WIDTH),
            |text| workbench_muted(workbench_mono(text)),
        )?));
    }
    let centred = StyledContainer::new(
        workbench_fill()
            .gap(WORKBENCH_GRID)
            .padding_all(WORKBENCH_GRID * 2.0)
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER),
        |_| RectStyle::default(),
        items,
    )?
    .role(Role::Status);
    Ok(box_item(centred))
}

fn spacer() -> Result<Box<dyn LayoutItem>, LayoutError> {
    Ok(box_item(StyledContainer::new(
        LayoutStyle::new().flex_grow(1.0),
        |_| RectStyle::default(),
        Vec::new(),
    )?))
}

fn fill() -> LayoutStyle {
    LayoutStyle::new()
        .flex_column()
        .flex_grow(1.0)
        .flex_basis(0.0)
        .min_width(0.0)
        .min_height(0.0)
}
