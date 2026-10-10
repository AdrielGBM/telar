use super::*;
use crate::preview::{Axis, Matrix, PreviewCtx};
use crate::{Container, LayoutError, LayoutItem, LayoutStyle};

fn empty(_: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    Ok(Box::new(Container::new(LayoutStyle::new(), Vec::new())?))
}

const MODES: Matrix = Matrix::Axes(&[Axis::Mode(&["light", "dark"])]);

fn entries() -> Vec<PreviewEntry> {
    vec![
        PreviewEntry::new("demo--card--plain", "card", "plain", empty),
        PreviewEntry::new("demo--card--themed", "card", "themed", empty).matrix(MODES),
        PreviewEntry::new("demo--badge--broken", "badge", "broken", empty)
            .matrix(Matrix::Named("nowhere")),
    ]
}

fn planned(filter: Option<&str>) -> Vec<String> {
    let entries = entries();
    let filter = filter.map(PreviewFilter::new);
    plan(&entries, &Matrices::default(), filter.as_ref())
        .into_iter()
        .flat_map(|(entry, units)| {
            units.into_iter().map(move |unit| match unit {
                Unit::Preview => entry.id.to_string(),
                Unit::Cell(cell) => cell.id,
                Unit::Unexpandable(_) => format!("{}--matrix", entry.id),
            })
        })
        .collect()
}

#[test]
fn every_preview_is_checked_as_it_opens_then_in_each_cell_of_its_matrix() {
    assert_eq!(
        planned(None),
        [
            "demo--card--plain",
            "demo--card--themed",
            "demo--card--themed--mode=light",
            "demo--card--themed--mode=dark",
            "demo--badge--broken",
            "demo--badge--broken--matrix",
        ]
    );
}

#[test]
fn a_filter_naming_a_preview_keeps_all_of_it() {
    assert_eq!(
        planned(Some("demo--card--themed")),
        [
            "demo--card--themed",
            "demo--card--themed--mode=light",
            "demo--card--themed--mode=dark",
        ]
    );
}

#[test]
fn a_filter_naming_only_cells_keeps_those_cells() {
    assert_eq!(
        planned(Some("*--mode=dark")),
        ["demo--card--themed--mode=dark"]
    );
}

#[test]
fn a_filter_naming_nothing_plans_nothing() {
    assert!(planned(Some("other--*")).is_empty());
}

#[test]
fn a_panic_becomes_the_message_it_panicked_with() {
    assert_eq!(guarded(|| 7), Ok(7));
    let panicked = guarded(|| -> () { panic!("the build broke") });
    assert_eq!(panicked, Err(String::from("panicked: the build broke")));
    let formatted = guarded(|| -> () { panic!("{} broke", "the layout") });
    assert_eq!(formatted, Err(String::from("panicked: the layout broke")));
}

fn greeting(_: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let text = crate::Text::declaring(|| String::from("Hello"), LayoutStyle::new(), |t| t)?;
    Ok(Box::new(Container::new(
        LayoutStyle::new(),
        vec![Box::new(text)],
    )?))
}

fn missing_lang(report: &CellReport) -> usize {
    report
        .a11y
        .violations
        .iter()
        .filter(|finding| finding.rule == "missing-lang")
        .count()
}

#[test]
fn a_run_names_the_language_of_a_canvas_that_names_none() {
    let entry = PreviewEntry::new("demo--greeting--plain", "greeting", "plain", greeting);
    assert_eq!(missing_lang(&check(&entry, None, A11yLevel::Warn, None)), 1);
    speak_the_app_language();
    assert_eq!(
        i18n_core::current_locale().as_deref(),
        Some(i18n_core::default_locale())
    );
    assert_eq!(missing_lang(&check(&entry, None, A11yLevel::Warn, None)), 0);
}

#[test]
fn a_run_keeps_the_language_the_app_chose() {
    i18n_core::set_locale("es");
    speak_the_app_language();
    assert_eq!(i18n_core::current_locale().as_deref(), Some("es"));
}
