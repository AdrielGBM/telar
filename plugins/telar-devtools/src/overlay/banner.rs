//! The build-error banner: shown over the app while the last rebuild failed, with the compiler's report in a code view.

use telar::{
    Accessible, AlignItems, Border, Children, Container, LayoutError, LayoutItem, LayoutStyle,
    Reactive, Role, ShapeStyle, StyledContainer, Text, box_item, memo, use_theme_tokens,
};
use telar_components::{CodeSpan, CodeViewProps, TokenKind, code_view};

use super::{Model, mounted_while};
use crate::strings::{self, BUILD_ERROR, BUILD_FAILED, BUILD_FAILED_HINT};
use crate::workbench::{WORKBENCH_GRID, use_workbench_tokens, workbench_card, workbench_muted};

const REPORT_MAX_HEIGHT: f32 = 280.0;
const LOCATION_ARROW: &str = "--> ";

/// Across the top of the app while the inspector is closed; with it open the banner lives in the room the drawer leaves ([`beside_drawer`]), so neither covers the other.
pub(super) fn banner(model: Model) -> Result<Box<dyn LayoutItem>, LayoutError> {
    mounted_while(
        LayoutStyle::new()
            .absolute()
            .inset_top(WORKBENCH_GRID)
            .inset_start(WORKBENCH_GRID)
            .inset_end(WORKBENCH_GRID),
        move || failed(model) && !model.inspector_open.get(),
        move || report(model),
    )
}

/// The banner as the top of the area right of the inspector drawer, where it is laid out with the drawer rather than over it.
pub(super) fn beside_drawer(model: Model) -> Result<Box<dyn LayoutItem>, LayoutError> {
    mounted_while(
        LayoutStyle::new(),
        move || failed(model),
        move || report(model),
    )
}

fn failed(model: Model) -> bool {
    model.build_error.with(Option::is_some)
}

fn report(model: Model) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let title = Text::declaring(
        || strings::text(BUILD_FAILED),
        LayoutStyle::new(),
        |text| {
            text.with_font_weight(600)
                .with_color(use_theme_tokens().error())
        },
    )?;
    let hint = Text::declaring(
        || strings::text(BUILD_FAILED_HINT),
        LayoutStyle::new(),
        workbench_muted,
    )?;
    let heading = Container::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .gap(WORKBENCH_GRID),
        vec![box_item(title), box_item(hint)],
    )?;
    let code = move || model.build_error.get().unwrap_or_default();
    let marked = memo(move || diagnostics(&code()));
    let report = code_view(
        CodeViewProps::props()
            .code(Reactive::of(code))
            .spans(Reactive::of(move || marked.get().0))
            .highlighted(Reactive::of(move || marked.get().1))
            .max_height(REPORT_MAX_HEIGHT)
            .label(Reactive::of(|| strings::text(BUILD_ERROR)))
            .build(),
        Children::default(),
    )?;
    let banner = StyledContainer::new(
        LayoutStyle::new()
            .flex_column()
            .gap(WORKBENCH_GRID)
            .padding_all(WORKBENCH_GRID * 1.5),
        |_| {
            workbench_card()
                .with_fill(use_workbench_tokens().panel_background)
                .with_border(Border::uniform(use_theme_tokens().error(), 1.0))
        },
        vec![box_item(heading), report],
    )?
    .input_opaque()
    .role(Role::Status)
    .a11y_label(|| strings::text(BUILD_FAILED));
    Ok(box_item(banner))
}

/// What the compiler's report points at: each `error` heading marked as one, its line highlighted, and each `-->` location set apart. Lines are numbered from 1, as the code view numbers them.
fn diagnostics(report: &str) -> (Vec<CodeSpan>, Vec<u32>) {
    let mut spans = Vec::new();
    let mut lines = Vec::new();
    let mut start = 0;
    for (index, line) in report.split('\n').enumerate() {
        if line.starts_with("error")
            && let Some(colon) = line.find(':')
        {
            spans.push(CodeSpan::new(start..start + colon, TokenKind::Error));
            lines.push(index as u32 + 1);
        } else if let Some(arrow) = line.find(LOCATION_ARROW) {
            let path = start + arrow + LOCATION_ARROW.len();
            spans.push(CodeSpan::new(
                path..start + line.trim_end().len(),
                TokenKind::Attribute,
            ));
        }
        start += line.len() + 1;
    }
    (spans, lines)
}

#[cfg(test)]
#[path = "banner_test.rs"]
mod tests;
