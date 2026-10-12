use std::rc::Rc;

use telar::preview::host::PreviewRequest;
use telar::preview::{ArgValue, PreviewCtx, PreviewEntry};
use telar::{
    Color, LayoutError, LayoutItem, LayoutStyle, Location, LocationFormat, Text, TextStyle,
    box_item, location_history, receive_location_history,
};

use super::*;
use crate::matrix::CustomAxis;
use crate::route::Page;
use crate::settings::CanvasSettings;
use crate::settings::ModeChoice;
use crate::state::ViewMode;

fn body(_: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    Ok(box_item(Text::new(
        || "body".to_string(),
        LayoutStyle::new(),
        || TextStyle::new(13.0, Color::BLACK),
    )?))
}

const PRIMARY: &str = "fake--button--primary";
const SECONDARY: &str = "fake--button--secondary";
const FIELD: &str = "fake--field--empty";

fn workshop() -> WorkshopState {
    let entries: Rc<[PreviewEntry]> = Rc::from(vec![
        PreviewEntry::new(PRIMARY, "button", "Primary", body).title("Inputs/Button"),
        PreviewEntry::new(SECONDARY, "button", "Secondary", body).title("Inputs/Button"),
        PreviewEntry::new(FIELD, "field", "Empty", body).title("Inputs/Field"),
    ]);
    let state = WorkshopState::new(entries, &PreviewRequest::default());
    follow(&state);
    state
}

fn at(reference: &str) -> Location {
    LocationFormat::root().parse(reference).unwrap()
}

fn shown() -> Vec<String> {
    location_history()
        .iter()
        .map(|location| LocationFormat::root().format(location))
        .collect()
}

#[test]
fn the_address_follows_the_selection_and_the_view() {
    let state = workshop();
    state.select(FIELD);
    assert_eq!(shown(), ["/preview/fake--field--empty"]);
    state.view().set(ViewMode::Docs);
    assert_eq!(shown(), ["/docs/Inputs/Field"]);
}

#[test]
fn a_link_opens_its_preview_with_its_args_and_settings_and_the_address_keeps_the_args() {
    let state = workshop();
    receive_location_history(vec![at(
        "/preview/fake--button--secondary?args=label:%22Hi%22&globals=locale:ar;grid",
    )]);
    assert_eq!(state.selection().peek().as_deref(), Some(SECONDARY));
    let entry = state.selected().unwrap();
    assert_eq!(
        state.args(&entry).overrides(),
        [("label".to_string(), ArgValue::Text("Hi".into()))]
    );
    assert_eq!(
        state.canvas_settings().peek(),
        CanvasSettings {
            locale: Some("ar".into()),
            grid: true,
            ..CanvasSettings::default()
        }
    );
    assert_eq!(
        shown(),
        ["/preview/fake--button--secondary?args=label:%22Hi%22"]
    );
}

#[test]
fn the_address_follows_the_args_as_they_are_edited_and_reset() {
    let state = workshop();
    let entry = state.selected().unwrap();
    let args = state.args(&entry);
    args.set("n", ArgValue::Int(2)).unwrap();
    assert_eq!(shown(), ["/preview/fake--button--primary?args=n:2"]);
    args.set("n", ArgValue::Int(3)).unwrap();
    assert_eq!(shown(), ["/preview/fake--button--primary?args=n:3"]);
    assert_eq!(args.overrides(), [("n".to_string(), ArgValue::Int(3))]);
    args.reset();
    assert_eq!(shown(), ["/preview/fake--button--primary"]);
}

#[test]
fn the_address_carries_the_matrix_view_s_axes_and_focused_cell() {
    let state = workshop();
    state.view().set(ViewMode::Matrix);
    assert_eq!(shown(), ["/matrix/fake--button--primary?axes="]);
    state
        .matrix_axes()
        .set(vec![CustomAxis::Mode, CustomAxis::Arg("tone".into())]);
    state
        .matrix_focus()
        .set(Some(format!("{PRIMARY}--mode=dark--tone=loud")));
    assert_eq!(
        shown(),
        ["/matrix/fake--button--primary?axes=mode;arg.tone&cell=mode%3Ddark--tone%3Dloud"]
    );
    state.view().set(ViewMode::Canvas);
    assert_eq!(shown(), ["/preview/fake--button--primary"]);
}

/// What `cargo telar preview --target web` relies on: the page reloads whole on every rebuild, and comes back from its address.
#[test]
fn a_workshop_started_at_its_address_comes_back_to_where_it_was() {
    let state = workshop();
    state.select(SECONDARY);
    let entry = state.selected().unwrap();
    state
        .args(&entry)
        .set("label", ArgValue::Text("Hi".into()))
        .unwrap();
    state.view().set(ViewMode::Matrix);
    state.matrix_axes().set(vec![CustomAxis::Direction]);
    state
        .matrix_focus()
        .set(Some(format!("{SECONDARY}--dir=rtl")));
    let address = shown();

    let reloaded = workshop();
    assert_eq!(shown(), address);
    assert_eq!(reloaded.selection().peek().as_deref(), Some(SECONDARY));
    assert_eq!(reloaded.view().peek(), ViewMode::Matrix);
    let entry = reloaded.selected().unwrap();
    assert_eq!(
        reloaded.args(&entry).overrides(),
        [("label".to_string(), ArgValue::Text("Hi".into()))]
    );
    assert_eq!(reloaded.matrix_axes().peek(), [CustomAxis::Direction]);
    assert_eq!(
        reloaded.matrix_focus().peek().as_deref(),
        Some("fake--button--secondary--dir=rtl")
    );
}

/// `cargo telar preview <ID>` names a preview and nothing set on it, which must not undo what the running workshop set there.
#[test]
fn an_address_naming_only_a_page_leaves_what_is_set_on_it() {
    let state = workshop();
    let entry = state.selected().unwrap();
    state.args(&entry).set("n", ArgValue::Int(2)).unwrap();
    state.select(FIELD);
    receive_location_history(vec![at("/preview/fake--button--primary")]);
    assert_eq!(state.selection().peek().as_deref(), Some(PRIMARY));
    assert_eq!(
        state.args(&entry).overrides(),
        [("n".to_string(), ArgValue::Int(2))]
    );
    assert_eq!(shown(), ["/preview/fake--button--primary?args=n:2"]);
}

#[test]
fn a_copied_link_opens_the_same_preview_args_and_settings() {
    let state = workshop();
    state.select(SECONDARY);
    let entry = state.selected().unwrap();
    state
        .args(&entry)
        .set("label", ArgValue::Text("Save; \"all\"".into()))
        .unwrap();
    state
        .canvas_settings()
        .update(|settings| settings.mode = ModeChoice::Dark);
    let link = state.link().unwrap();

    let other = workshop();
    receive_location_history(vec![at(&link)]);
    assert_eq!(other.selection().peek().as_deref(), Some(SECONDARY));
    let entry = other.selected().unwrap();
    assert_eq!(
        other.args(&entry).overrides(),
        [("label".to_string(), ArgValue::Text("Save; \"all\"".into()))]
    );
    assert_eq!(other.canvas_settings().peek().mode, ModeChoice::Dark);
}

#[test]
fn chrome_0_shows_the_canvas_alone() {
    let state = workshop();
    receive_location_history(vec![at("/preview/fake--field--empty?chrome=0")]);
    assert!(!state.chrome().peek());
    assert_eq!(shown(), ["/preview/fake--field--empty?chrome=0"]);
}

#[test]
fn a_docs_address_selects_the_first_preview_of_its_component() {
    let state = workshop();
    receive_location_history(vec![at("/docs/Inputs/Field")]);
    assert_eq!(state.selection().peek().as_deref(), Some(FIELD));
    assert_eq!(state.view().peek(), ViewMode::Docs);
}

#[test]
fn an_address_naming_no_preview_settles_on_where_the_workshop_is() {
    let state = workshop();
    receive_location_history(vec![at("/preview/gone--card--a")]);
    assert_eq!(state.selection().peek().as_deref(), Some(PRIMARY));
    assert_eq!(shown(), ["/preview/fake--button--primary"]);
}

#[test]
fn a_title_with_spaces_around_its_slash_is_opened_by_either_spelling_and_written_canonically() {
    let entries: Rc<[PreviewEntry]> = Rc::from(vec![
        PreviewEntry::new(FIELD, "field", "Empty", body).title("Inputs/Field"),
        PreviewEntry::new(PRIMARY, "button", "Primary", body).title("Forms / Button"),
    ]);
    let state = WorkshopState::new(entries, &PreviewRequest::default());
    follow(&state);
    for reference in ["/docs/Forms/Button", "/docs/Forms/Button/"] {
        state.select(FIELD);
        receive_location_history(vec![at(reference)]);
        assert_eq!(
            state.selection().peek().as_deref(),
            Some(PRIMARY),
            "{reference}"
        );
        assert_eq!(state.view().peek(), ViewMode::Docs);
        assert_eq!(
            shown().last().map(String::as_str),
            Some("/docs/Forms/Button")
        );
    }
    state.select(FIELD);
    let spaced = WorkshopRoute::new(Page::Docs("Forms / Button".into()), true);
    state.open(&spaced);
    assert_eq!(state.selection().peek().as_deref(), Some(PRIMARY));
    assert_eq!(
        state.current_route().map(|route| route.page),
        Some(Page::Docs("Forms/Button".into()))
    );
}
