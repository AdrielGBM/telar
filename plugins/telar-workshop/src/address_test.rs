use std::rc::Rc;

use telar::preview::host::PreviewRequest;
use telar::preview::{ArgValue, PreviewCtx, PreviewEntry};
use telar::{
    Color, LayoutError, LayoutItem, LayoutStyle, Location, LocationFormat, Text, TextStyle,
    box_item, location_history, receive_location_history,
};

use super::*;
use crate::settings::CanvasSettings;
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
fn a_link_opens_its_preview_with_its_args_and_settings_and_the_address_keeps_neither() {
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
    assert_eq!(shown(), ["/preview/fake--button--secondary"]);
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
        .update(|settings| settings.mode = Some("dark".into()));
    let link = state.link().unwrap();

    let other = workshop();
    receive_location_history(vec![at(&link)]);
    assert_eq!(other.selection().peek().as_deref(), Some(SECONDARY));
    let entry = other.selected().unwrap();
    assert_eq!(
        other.args(&entry).overrides(),
        [("label".to_string(), ArgValue::Text("Save; \"all\"".into()))]
    );
    assert_eq!(other.canvas_settings().peek().mode.as_deref(), Some("dark"));
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
