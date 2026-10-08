//! What the canvas shows in place of a preview: that there are none, or the card for one that failed.

use std::rc::Rc;

use telar::preview::PreviewEntry;
use telar::{
    AlignItems, Border, BorderRadius, BuildFailure, Children, JustifyContent, LayoutError,
    LayoutItem, LayoutStyle, Reactive, RectStyle, Role, ShapeStyle, StyledContainer, Text,
    TextStyle, box_item, open_uri, use_theme_tokens,
};
use telar_components::{ButtonProps, button};
use telar_devtools::{WORKBENCH_GRID, WORKBENCH_RADIUS, use_workbench_tokens, workbench_scope};

use super::remount_button;
use crate::state::WorkshopState;
use crate::strings::{self, BUILD_FAILED, NO_PREVIEWS, NO_PREVIEWS_HINT, OPEN_IN_EDITOR, PANICKED};

const CARD_MAX_WIDTH: f32 = 480.0;

pub(super) fn empty() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let title = Text::declaring(
        || strings::text(NO_PREVIEWS),
        LayoutStyle::new(),
        |text| text.with_font_weight(600),
    )?;
    let hint = Text::declaring(
        || strings::text(NO_PREVIEWS_HINT),
        LayoutStyle::new().max_width(CARD_MAX_WIDTH),
        |text| text.with_color(use_workbench_tokens().text_muted),
    )?;
    let centred = StyledContainer::new(
        LayoutStyle::new()
            .flex_column()
            .flex_grow(1.0)
            .min_height(0.0)
            .gap(WORKBENCH_GRID)
            .padding_all(WORKBENCH_GRID * 2.0)
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER),
        |_| RectStyle::default(),
        vec![box_item(title), box_item(hint)],
    )?
    .role(Role::Status);
    Ok(box_item(centred))
}

/// The card a preview's canvas shows when building it failed: what went wrong, where the preview is written, and a way to open it there or, after a panic, to mount it afresh.
///
/// Built inside the canvas, in the workbench theme rather than the application's: the card is the workshop speaking.
pub(super) fn failure(
    state: &WorkshopState,
    entry: &PreviewEntry,
    failure: BuildFailure,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let (state, entry) = (state.clone(), *entry);
    Ok(box_item(workbench_scope(move || {
        card(&state, &entry, failure)
    })?))
}

fn card(
    state: &WorkshopState,
    entry: &PreviewEntry,
    failure: BuildFailure,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let (heading, detail, panicked) = match failure {
        BuildFailure::Failed(err) => (BUILD_FAILED, err.to_string(), false),
        BuildFailure::Panicked(message) => (PANICKED, message, true),
    };
    let heading = Text::declaring(
        move || strings::text(heading),
        LayoutStyle::new(),
        |text| {
            text.with_font_weight(600)
                .with_color(use_theme_tokens().error())
        },
    )?;
    let detail = Text::declaring(move || detail.clone(), LayoutStyle::new(), mono)?;
    let mut items = vec![box_item(heading), box_item(detail)];
    let mut actions = Vec::new();
    if let Some(location) = location(entry) {
        items.push(box_item(Text::declaring(
            move || location.clone(),
            LayoutStyle::new(),
            |text| mono(text).with_color(use_workbench_tokens().text_muted),
        )?));
        actions.push(open_in_editor(entry.file, entry.line)?);
    }
    if panicked {
        actions.push(remount_button(state)?);
    }
    if !actions.is_empty() {
        items.push(box_item(StyledContainer::new(
            LayoutStyle::new().flex_row().gap(WORKBENCH_GRID),
            |_| RectStyle::default(),
            actions,
        )?));
    }
    let card = StyledContainer::new(
        LayoutStyle::new()
            .flex_column()
            .gap(WORKBENCH_GRID)
            .padding_all(WORKBENCH_GRID * 2.0)
            .max_width(CARD_MAX_WIDTH),
        |_| {
            let tokens = use_workbench_tokens();
            RectStyle::default()
                .with_fill(tokens.panel_background)
                .with_border(Border::uniform(tokens.border_subtle, 1.0))
                .with_radius(BorderRadius::all(WORKBENCH_RADIUS))
        },
        items,
    )?
    .role(Role::Status);
    Ok(box_item(card))
}

/// `file:line`, or `None` when the preview's file is unknown.
fn location(entry: &PreviewEntry) -> Option<String> {
    match (entry.file, entry.line) {
        ("", _) => None,
        (file, 0) => Some(file.to_owned()),
        (file, line) => Some(format!("{file}:{line}")),
    }
}

fn open_in_editor(file: &'static str, line: u32) -> Result<Box<dyn LayoutItem>, LayoutError> {
    button(
        ButtonProps::props()
            .label(Reactive::of(|| strings::text(OPEN_IN_EDITOR)))
            .on_press(Rc::new(move || {
                open_uri(&editor_uri(file, line));
            }))
            .build(),
        Children::default(),
    )
}

/// The URI that opens `file` at `line` in the editor the system hands it to, the path percent-encoded and with forward slashes on every platform.
pub(super) fn editor_uri(file: &str, line: u32) -> String {
    let path = file.replace('\\', "/");
    let mut encoded = String::with_capacity(path.len());
    for byte in path.trim_start_matches('/').bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' | b':' => {
                encoded.push(byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    match line {
        0 => format!("vscode://file/{encoded}"),
        line => format!("vscode://file/{encoded}:{line}"),
    }
}

fn mono(text: TextStyle) -> TextStyle {
    let tokens = use_workbench_tokens();
    text.with_font_family(tokens.mono_family)
        .with_font_size(tokens.mono_size)
}
