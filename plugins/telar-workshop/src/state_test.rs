use std::collections::HashMap;

use telar::preview::host::PreviewRequest;
use telar::preview::{ArgValue, PreviewCtx};
use telar::{
    Color, LayoutError, LayoutItem, LayoutStyle, Text, TextStyle, box_item, effect,
    hot_restore_json, hot_snapshot_json,
};

use super::*;
use crate::settings::{CanvasSize, Zoom};

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

fn entries() -> Rc<[PreviewEntry]> {
    Rc::from(vec![
        PreviewEntry::new(PRIMARY, "button", "Primary", body).title("Inputs/Button"),
        PreviewEntry::new(SECONDARY, "button", "Secondary", body).title("Inputs/Button"),
        PreviewEntry::new(FIELD, "field", "Empty", body).title("Inputs/Field"),
    ])
}

fn restore(values: &[(&str, &str)]) {
    let map: HashMap<&str, &str> = values.iter().copied().collect();
    hot_restore_json(&serde_json::to_string(&map).unwrap());
}

fn sorted(set: &HashSet<Arc<str>>) -> Vec<&str> {
    let mut ids: Vec<&str> = set.iter().map(|id| &**id).collect();
    ids.sort_unstable();
    ids
}

#[test]
fn the_hot_signal_keys_are_stable() {
    assert_eq!(keys::SELECTION, "@workshop/selection");
    assert_eq!(keys::VIEW, "@workshop/view");
    assert_eq!(keys::SEARCH, "@workshop/search");
    assert_eq!(keys::SIDEBAR_EXPANDED, "@workshop/sidebar.expanded");
    assert_eq!(keys::SIDEBAR_WIDTH, "@workshop/sidebar.width");
    assert_eq!(keys::SIDEBAR_COLLAPSED, "@workshop/sidebar.collapsed");
    assert_eq!(keys::PANEL_SIZE, "@workshop/panel.size");
    assert_eq!(keys::PANEL_COLLAPSED, "@workshop/panel.collapsed");
    assert_eq!(keys::PANEL_POSITION, "@workshop/panel.position");
    assert_eq!(keys::CANVAS_SETTINGS, "@workshop/canvas.settings");
}

#[test]
fn a_snapshot_carries_every_key() {
    let _state = WorkshopState::new(entries(), &PreviewRequest::default());
    let snapshot: HashMap<String, String> = serde_json::from_str(&hot_snapshot_json()).unwrap();
    for key in [
        keys::SELECTION,
        keys::VIEW,
        keys::SEARCH,
        keys::SIDEBAR_EXPANDED,
        keys::SIDEBAR_WIDTH,
        keys::SIDEBAR_COLLAPSED,
        keys::PANEL_SIZE,
        keys::PANEL_COLLAPSED,
        keys::PANEL_POSITION,
        keys::CANVAS_SETTINGS,
    ] {
        assert!(snapshot.contains_key(key), "{key} is not in {snapshot:?}");
    }
}

#[test]
fn the_selection_defaults_to_the_first_preview() {
    let state = WorkshopState::new(entries(), &PreviewRequest::default());
    assert_eq!(state.selection().peek().as_deref(), Some(PRIMARY));
}

#[test]
fn nothing_is_selected_without_previews() {
    let state = WorkshopState::new(Rc::from(Vec::new()), &id(PRIMARY));
    assert_eq!(state.selection().peek(), None);
    assert!(state.selected().is_none());
}

#[test]
fn a_requested_id_wins() {
    let state = WorkshopState::new(entries(), &id(SECONDARY));
    assert_eq!(state.selected().map(|entry| entry.id), Some(SECONDARY));
}

#[test]
fn an_unknown_request_falls_back_to_the_first_preview() {
    let state = WorkshopState::new(entries(), &id("nope"));
    assert_eq!(state.selection().peek().as_deref(), Some(PRIMARY));
}

#[test]
fn a_restored_selection_wins_over_the_request() {
    restore(&[
        (keys::SELECTION, "\"fake--field--empty\""),
        (keys::SIDEBAR_WIDTH, "320.0"),
        (keys::SIDEBAR_COLLAPSED, "true"),
        (keys::PANEL_COLLAPSED, "true"),
        (keys::PANEL_POSITION, "\"Right\""),
        (keys::VIEW, "\"Docs\""),
        (keys::SEARCH, "\"butt\""),
    ]);
    let state = WorkshopState::new(entries(), &id(SECONDARY));
    assert_eq!(state.selection().peek().as_deref(), Some(FIELD));
    assert_eq!(state.sidebar_width().peek(), 320.0);
    assert!(state.sidebar_collapsed().peek());
    assert!(state.panel_collapsed().peek());
    assert_eq!(state.panel_position().peek(), PanelPosition::Right);
    assert_eq!(state.view().peek(), ViewMode::Docs);
    assert_eq!(state.search().peek(), "butt");
}

#[test]
fn a_restored_selection_no_preview_has_gives_way_to_the_request() {
    restore(&[(keys::SELECTION, "\"fake--gone--away\"")]);
    let state = WorkshopState::new(entries(), &id(SECONDARY));
    assert_eq!(state.selection().peek().as_deref(), Some(SECONDARY));
}

#[test]
fn selecting_an_unknown_id_keeps_the_selection() {
    let state = WorkshopState::new(entries(), &PreviewRequest::default());
    assert!(!state.select("nope"));
    assert_eq!(state.selection().peek().as_deref(), Some(PRIMARY));
    assert!(state.select(FIELD));
    assert_eq!(state.selection().peek().as_deref(), Some(FIELD));
}

#[test]
fn every_group_starts_open() {
    let state = WorkshopState::new(entries(), &PreviewRequest::default());
    assert_eq!(
        sorted(&state.expanded().peek()),
        ["/Inputs", "/Inputs/Button", "/Inputs/Field"]
    );
}

#[test]
fn a_restored_set_of_open_groups_wins() {
    restore(&[(keys::SIDEBAR_EXPANDED, r#"["/Inputs"]"#)]);
    let state = WorkshopState::new(entries(), &PreviewRequest::default());
    assert_eq!(sorted(&state.expanded().peek()), ["/Inputs"]);
}

#[test]
fn the_canvas_settings_start_at_their_defaults_and_are_restored_whole() {
    assert_eq!(
        WorkshopState::new(entries(), &PreviewRequest::default())
            .canvas_settings()
            .peek(),
        CanvasSettings::default()
    );
    restore(&[(
        keys::CANVAS_SETTINGS,
        r#"{"locale":"ar","size":{"custom":{"width":320.0,"height":480.0}},"zoom":{"percent":50}}"#,
    )]);
    let state = WorkshopState::new(entries(), &PreviewRequest::default());
    assert_eq!(
        state.canvas_settings().peek(),
        CanvasSettings {
            locale: Some("ar".into()),
            size: CanvasSize::Custom {
                width: 320.0,
                height: 480.0
            },
            zoom: Zoom::Percent(50),
            ..CanvasSettings::default()
        }
    );
}

#[test]
fn a_remount_is_counted_and_tracked() {
    let state = WorkshopState::new(entries(), &PreviewRequest::default());
    let seen = signal(Vec::new());
    let tracking = state.clone();
    effect(move || {
        let count = tracking.remounts();
        seen.update(|seen| seen.push(count));
    });
    state.remount();
    state.remount();
    assert_eq!(seen.peek(), [0, 1, 2]);
}

#[test]
fn a_remount_count_is_not_carried_across_a_reload() {
    let state = WorkshopState::new(entries(), &PreviewRequest::default());
    state.remount();
    let snapshot = hot_snapshot_json();
    hot_restore_json(&snapshot);
    assert_eq!(
        WorkshopState::new(entries(), &PreviewRequest::default()).remounts(),
        0
    );
}

#[test]
fn each_preview_has_one_args_handle() {
    let state = WorkshopState::new(entries(), &PreviewRequest::default());
    let [primary, secondary] = [state.entries()[0], state.entries()[1]];
    state
        .args(&primary)
        .set("label", ArgValue::Text("Edited".into()))
        .unwrap();
    assert_eq!(
        state.args(&primary).overrides(),
        vec![("label".to_string(), ArgValue::Text("Edited".into()))]
    );
    assert!(state.args(&secondary).overrides().is_empty());
}

#[test]
fn args_outlive_the_scope_that_first_asked_for_them() {
    let state = WorkshopState::new(entries(), &PreviewRequest::default());
    let entry = state.entries()[0];
    let scope = telar::owner_scope();
    let owner = scope.id();
    let args = state.args(&entry);
    drop(scope);
    telar::dispose_owner(owner);
    args.set("label", ArgValue::Text("Edited".into())).unwrap();
    assert_eq!(args.remounts(), state.args(&entry).remounts());
}

fn id(id: &str) -> PreviewRequest {
    PreviewRequest::new(Some(id), None)
}

fn component(component: &str) -> PreviewRequest {
    PreviewRequest::new(None, Some(component))
}

#[test]
fn a_requested_component_selects_its_first_preview() {
    let state = WorkshopState::new(entries(), &component("field"));
    assert_eq!(state.selection().peek().as_deref(), Some(FIELD));
    let state = WorkshopState::new(entries(), &component("Button"));
    assert_eq!(state.selection().peek().as_deref(), Some(PRIMARY));
}

#[test]
fn an_unknown_component_falls_back_to_the_first_preview() {
    let state = WorkshopState::new(entries(), &component("nope"));
    assert_eq!(state.selection().peek().as_deref(), Some(PRIMARY));
}

#[test]
fn a_requested_id_beats_a_requested_component() {
    let request = PreviewRequest::new(Some(SECONDARY), Some("field"));
    let state = WorkshopState::new(entries(), &request);
    assert_eq!(state.selection().peek().as_deref(), Some(SECONDARY));
}

#[test]
fn a_restored_selection_beats_a_requested_id_and_component() {
    restore(&[(keys::SELECTION, "\"fake--button--secondary\"")]);
    let request = PreviewRequest::new(Some(FIELD), Some("field"));
    let state = WorkshopState::new(entries(), &request);
    assert_eq!(state.selection().peek().as_deref(), Some(SECONDARY));
}
