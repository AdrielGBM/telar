//! The banner over the app while it is not running what was last saved: a rebuild that failed, with the compiler's report, a build that compiled but that the app would not load, with why, or a build that panicked, with what it said and where.

use telar::{
    Accessible, AlignItems, Border, BuildOutcome, Children, Container, LayoutError, LayoutItem,
    LayoutStyle, Reactive, Role, ShapeStyle, StyledContainer, Text, box_item, children, memo,
    use_theme_tokens,
};
use telar_components::{CodeSpan, CodeViewProps, TokenKind, code_view};

use super::{Model, mounted_while};
use crate::strings::{
    self, APP_PANIC, APP_PANICKED, APP_PANICKED_HINT, BUILD_ERROR, BUILD_FAILED, BUILD_FAILED_HINT,
    REFUSAL_REASON, RELOAD_REFUSED, RELOAD_REFUSED_HINT,
};
use crate::workbench::{WORKBENCH_GRID, use_workbench_tokens, workbench_card, workbench_muted};

const REPORT_MAX_HEIGHT: f32 = 280.0;
const LOCATION_ARROW: &str = "--> ";

/// One report the banner can carry, and the words that say which kind it is.
#[derive(Clone, Copy)]
struct Report {
    model: Model,
    read: fn(Model) -> Option<String>,
    title: &'static str,
    hint: &'static str,
    label: &'static str,
}

impl Report {
    fn text(self) -> Option<String> {
        (self.read)(self.model)
    }
}

/// The panic first: while there is one, nothing of the app is running at all.
fn reports(model: Model) -> [Report; 3] {
    [
        Report {
            model,
            read: |model| model.app_panic.get(),
            title: APP_PANICKED,
            hint: APP_PANICKED_HINT,
            label: APP_PANIC,
        },
        Report {
            model,
            read: |model| {
                model.build.with(|build| match build {
                    BuildOutcome::Refused(reason) => Some(reason.clone()),
                    _ => None,
                })
            },
            title: RELOAD_REFUSED,
            hint: RELOAD_REFUSED_HINT,
            label: REFUSAL_REASON,
        },
        Report {
            model,
            read: |model| {
                model.build.with(|build| match build {
                    BuildOutcome::Failed(report) => Some(report.clone()),
                    _ => None,
                })
            },
            title: BUILD_FAILED,
            hint: BUILD_FAILED_HINT,
            label: BUILD_ERROR,
        },
    ]
}

/// Across the top of the app while the inspector is closed; with it open the banner lives in the room the drawer leaves ([`beside_drawer`]), so neither covers the other.
pub(super) fn banner(model: Model) -> Result<Box<dyn LayoutItem>, LayoutError> {
    mounted_while(
        LayoutStyle::new()
            .absolute()
            .inset_top(WORKBENCH_GRID)
            .inset_start(WORKBENCH_GRID)
            .inset_end(WORKBENCH_GRID),
        move || failed(model) && !model.inspector_open.get(),
        move || cards(model),
    )
}

/// The banner as the top of the area right of the inspector drawer, where it is laid out with the drawer rather than over it.
pub(super) fn beside_drawer(model: Model) -> Result<Box<dyn LayoutItem>, LayoutError> {
    mounted_while(
        LayoutStyle::new(),
        move || failed(model),
        move || cards(model),
    )
}

fn failed(model: Model) -> bool {
    reports(model).iter().any(|report| report.text().is_some())
}

fn cards(model: Model) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let cards = reports(model)
        .into_iter()
        .map(|kind| {
            mounted_while(
                LayoutStyle::new(),
                move || kind.text().is_some(),
                move || card(kind),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let column = Container::new(LayoutStyle::column().gap(WORKBENCH_GRID), cards)?;
    Ok(box_item(column))
}

fn card(kind: Report) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let title = Text::declaring(
        move || strings::text(kind.title),
        LayoutStyle::new(),
        |text| {
            text.with_font_weight(600)
                .with_color(use_theme_tokens().error())
        },
    )?;
    let hint = Text::declaring(
        move || strings::text(kind.hint),
        LayoutStyle::new(),
        workbench_muted,
    )?;
    let heading = Container::new(
        LayoutStyle::row()
            .align_items(AlignItems::CENTER)
            .gap(WORKBENCH_GRID),
        children![title, hint],
    )?;
    let code = move || kind.text().unwrap_or_default();
    let marked = memo(move || diagnostics(&code()));
    let report = code_view(
        CodeViewProps::props()
            .code(Reactive::of(code))
            .spans(Reactive::of(move || marked.get().0))
            .highlighted(Reactive::of(move || marked.get().1))
            .max_height(REPORT_MAX_HEIGHT)
            .label(Reactive::of(move || strings::text(kind.label)))
            .build(),
        Children::default(),
    )?;
    let banner = StyledContainer::new(
        LayoutStyle::column()
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
    .a11y_label(move || strings::text(kind.title));
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
