use telar::preview::host::PreviewRequest;
use telar::preview::{ArgValue, PreviewCtx};
use telar::testing::{find_text, mount, rect_of};
use telar::{
    AppRuntime, Color, ComponentList, LayoutError, LayoutItem, LayoutStyle, LocalApp, Text,
    TextStyle, box_item, relayout_if_dirty,
};

use super::*;
use crate::state::{
    PANEL_DEFAULT_SIZE, PANEL_MAX_FRACTION, PANEL_MIN_SIZE, SIDEBAR_DEFAULT_WIDTH,
    SIDEBAR_MAX_WIDTH, SIDEBAR_MIN_WIDTH,
};

const WIDTH: u32 = 1200;
const HEIGHT: u32 = 800;

fn label(ctx: &PreviewCtx, default: &'static str) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let text = ctx.arg("label", default.to_string());
    Ok(box_item(Text::new(
        move || text.clone(),
        LayoutStyle::new(),
        || TextStyle::new(13.0, Color::BLACK),
    )?))
}

fn entries() -> Vec<PreviewEntry> {
    vec![
        PreviewEntry::new("fake--button--primary", "button", "Primary", |ctx| {
            label(ctx, "primary body")
        }),
        PreviewEntry::new("fake--button--secondary", "button", "Secondary", |ctx| {
            label(ctx, "secondary body")
        }),
    ]
}

fn width_of(tree: &ComponentList, text: &str) -> f32 {
    rect_of(tree, text)
        .unwrap_or_else(|| panic!("{text} is not drawn"))
        .width
}

fn lay_out(app: &WorkshopApp) -> ComponentList {
    let tree = mount(app.root(), WIDTH, HEIGHT);
    relayout_if_dirty();
    tree
}

/// The width of the region announced as `name`, read from the access tree: what a region draws need not span it.
fn region_width(runtime: &LocalApp<WorkshopApp>, tree: &ComponentList, name: &str) -> f32 {
    region(runtime, tree, name).width
}

fn region(runtime: &LocalApp<WorkshopApp>, tree: &ComponentList, name: &str) -> telar::Rect {
    runtime
        .access_snapshot(&tree.commands())
        .into_iter()
        .find(|node| node.name == name)
        .unwrap_or_else(|| panic!("no region is announced as {name}"))
        .rect
}

#[test]
fn the_shell_lays_out_every_region() {
    let runtime = LocalApp(WorkshopApp::new(entries()).project_name("bench"));
    let tree = lay_out(&runtime.0);
    for text in ["bench", "Search previews", "Canvas", "Previews", "Controls"] {
        assert!(find_text(&tree, text), "{text} is not drawn");
    }
    assert_eq!(width_of(&tree, "Previews"), SIDEBAR_DEFAULT_WIDTH);
    assert_eq!(
        region_width(&runtime, &tree, "Panels"),
        WIDTH as f32 - SIDEBAR_DEFAULT_WIDTH - region_width(&runtime, &tree, "Resize the sidebar")
    );
}

#[test]
fn only_the_selected_preview_mounts() {
    let tree = lay_out(&WorkshopApp::new(entries()));
    assert!(find_text(&tree, "primary body"));
    assert!(!find_text(&tree, "secondary body"));
}

#[test]
fn a_requested_preview_mounts_instead() {
    let tree = lay_out(&WorkshopApp::new(entries()).select("fake--button--secondary"));
    assert!(find_text(&tree, "secondary body"));
    assert!(!find_text(&tree, "primary body"));
}

#[test]
fn the_canvas_builds_with_the_args_the_state_holds() {
    reset_layout_runtime();
    let state = WorkshopState::new(entries().into(), &PreviewRequest::default());
    let canvas = crate::canvas::canvas(&state).unwrap();
    let tree = mount(WindowRoot::wrapping(canvas).unwrap(), WIDTH, HEIGHT);
    relayout_if_dirty();
    assert!(find_text(&tree, "primary body"));
    state
        .args(&state.entries()[0])
        .set("label", ArgValue::Text("edited body".into()))
        .unwrap();
    relayout_if_dirty();
    assert!(find_text(&tree, "edited body"));
}

#[test]
fn an_empty_workshop_says_so() {
    let tree = lay_out(&WorkshopApp::new(Vec::new()));
    assert!(find_text(&tree, "No previews"));
}

#[test]
fn panels_on_the_right_take_their_size_in_width() {
    telar::hot_restore_json(
        r#"{"@workshop/panel.position":"\"Right\"","@workshop/sidebar.width":"300.0"}"#,
    );
    let runtime = LocalApp(WorkshopApp::new(entries()));
    let tree = lay_out(&runtime.0);
    assert_eq!(width_of(&tree, "Previews"), 300.0);
    assert_eq!(region_width(&runtime, &tree, "Panels"), PANEL_DEFAULT_SIZE);
}

#[test]
fn a_sidebar_width_out_of_range_is_kept_within_it() {
    telar::hot_restore_json(r#"{"@workshop/sidebar.width":"40.0"}"#);
    let tree = lay_out(&WorkshopApp::new(entries()));
    assert_eq!(width_of(&tree, "Previews"), SIDEBAR_MIN_WIDTH);
}

fn splitter(
    runtime: &LocalApp<WorkshopApp>,
    tree: &ComponentList,
    name: &str,
) -> telar::AccessNode {
    runtime
        .access_snapshot(&tree.commands())
        .into_iter()
        .find(|node| node.role == telar::Role::Splitter && node.name == name)
        .unwrap_or_else(|| panic!("no splitter is announced as {name}"))
}

#[test]
fn each_splitter_is_named_and_reports_the_room_its_pane_has() {
    let runtime = LocalApp(WorkshopApp::new(entries()));
    let tree = lay_out(&runtime.0);
    let sidebar = splitter(&runtime, &tree, "Resize the sidebar")
        .value
        .expect("the sidebar's splitter carries its width");
    assert_eq!(
        (sidebar.now, sidebar.min, sidebar.max),
        (
            f64::from(SIDEBAR_DEFAULT_WIDTH),
            f64::from(SIDEBAR_MIN_WIDTH),
            f64::from(SIDEBAR_MAX_WIDTH)
        )
    );
    let panels = splitter(&runtime, &tree, "Resize the panels")
        .value
        .expect("the panels' splitter carries their size");
    assert_eq!(panels.now, f64::from(PANEL_DEFAULT_SIZE));
    assert_eq!(panels.min, f64::from(PANEL_MIN_SIZE));
    let shared = splitter(&runtime, &tree, "Resize the sidebar").rect.height;
    assert!(
        (panels.max - f64::from(shared * PANEL_MAX_FRACTION)).abs() < 0.5,
        "{} is not {PANEL_MAX_FRACTION} of {shared}",
        panels.max
    );
}

#[test]
fn collapsed_panes_keep_their_sizes_for_when_they_open() {
    telar::hot_restore_json(
        r#"{"@workshop/sidebar.collapsed":"true","@workshop/panel.collapsed":"true","@workshop/sidebar.width":"300.0"}"#,
    );
    let runtime = LocalApp(WorkshopApp::new(entries()));
    let tree = lay_out(&runtime.0);
    let announced = runtime.access_snapshot(&tree.commands());
    assert!(
        !announced.iter().any(|node| node.name == "Previews"),
        "a folded sidebar is not announced"
    );
    assert_eq!(runtime.0.entries.len(), 2);
    assert_eq!(
        splitter(&runtime, &tree, "Resize the panels")
            .value
            .unwrap()
            .now,
        0.0
    );
    assert!(
        find_text(&tree, "primary body"),
        "the canvas takes the room"
    );
}

#[test]
#[should_panic(expected = "preview id `fake--button--primary` names 2 previews")]
fn previews_sharing_an_id_are_reported() {
    let mut entries = entries();
    entries.push(entries[0]);
    let _ = WorkshopApp::new(entries);
}

#[test]
fn a_requested_component_mounts_its_first_preview() {
    let mut app = WorkshopApp::new(vec![
        PreviewEntry::new("fake--field--a", "field", "A", |ctx| {
            label(ctx, "field body")
        }),
        PreviewEntry::new("fake--button--a", "button", "A", |ctx| {
            label(ctx, "button body")
        }),
    ]);
    app.requested = PreviewRequest::new(None, Some("button"));
    let tree = lay_out(&app);
    assert!(find_text(&tree, "button body"));
    assert!(!find_text(&tree, "field body"));
}

#[test]
fn a_requested_id_beats_a_requested_component_on_the_app() {
    let mut app = WorkshopApp::new(vec![
        PreviewEntry::new("fake--field--a", "field", "A", |ctx| {
            label(ctx, "field body")
        }),
        PreviewEntry::new("fake--button--a", "button", "A", |ctx| {
            label(ctx, "button body")
        }),
    ]);
    app.requested = PreviewRequest::new(Some("fake--field--a"), Some("button"));
    let tree = lay_out(&app);
    assert!(find_text(&tree, "field body"));
}
