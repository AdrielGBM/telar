use std::collections::HashSet;

use telar::preview::host::Args;
use telar::preview::{PreviewCtx, PreviewEntry};
use telar::{AvailableSpace, Event, PointerButton, PointerSource, compute_layout, track_layout};

use super::previews;

fn mount(entry: &PreviewEntry) {
    crate::test_support::fresh_layout_runtime();
    let ctx = PreviewCtx::from(Args::for_entry(entry));
    let root = entry
        .build_root(&ctx)
        .unwrap_or_else(|error| panic!("`{}` does not build: {error:?}", entry.id));
    compute_layout(
        root.layout_node(),
        AvailableSpace::Definite(480.0),
        AvailableSpace::Definite(360.0),
    )
    .unwrap_or_else(|error| panic!("`{}` does not lay out: {error:?}", entry.id));
}

#[test]
fn every_preview_builds_headlessly() {
    for entry in previews() {
        mount(&entry);
    }
}

#[test]
fn every_preview_has_its_own_id() {
    let entries = previews();
    let ids: HashSet<&str> = entries.iter().map(|entry| entry.id).collect();
    assert_eq!(ids.len(), entries.len());
}

/// Building blocks called with plain arguments rather than as a tag with derived props.
const WITHOUT_PROPS: [&str; 2] = ["line_gutter", "window_frame"];

#[test]
fn every_preview_describes_its_component_by_its_props() {
    for entry in previews()
        .into_iter()
        .filter(|entry| !WITHOUT_PROPS.contains(&entry.component))
    {
        let schema = entry
            .props
            .unwrap_or_else(|| panic!("`{}` names no props", entry.id))();
        assert!(!schema.fields.is_empty(), "`{}`", entry.id);
    }
}

fn button_preview(name: &str) -> PreviewEntry {
    previews()
        .into_iter()
        .find(|entry| entry.component == "button" && entry.name == name)
        .unwrap_or_else(|| panic!("no button preview named {name}"))
}

/// Builds `entry` against a fresh canvas context and taps the middle of its root, as a pointer would.
fn tap(entry: &PreviewEntry) -> PreviewCtx {
    crate::test_support::fresh_layout_runtime();
    let ctx = PreviewCtx::from(Args::for_entry(entry));
    let mut root = entry.build_root(&ctx).expect("the preview builds");
    let rect = track_layout(root.layout_node()).expect("the root has a layout node");
    compute_layout(
        root.layout_node(),
        AvailableSpace::Definite(480.0),
        AvailableSpace::Definite(360.0),
    )
    .expect("the preview lays out");
    let rect = rect.get();
    let (x, y) = (
        f64::from(rect.x + rect.width / 2.0),
        f64::from(rect.y + rect.height / 2.0),
    );
    for event in [
        Event::PointerPressed {
            x,
            y,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        },
        Event::PointerReleased {
            x,
            y,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        },
    ] {
        root.on_event(&event);
    }
    ctx
}

#[test]
fn pressing_a_previewed_button_logs_on_press() {
    let ctx = tap(&button_preview("Default"));
    let calls = ctx.actions().calls();
    let names: Vec<&str> = calls.iter().map(|call| call.name).collect();
    assert_eq!(names, ["on_press"]);
    assert!(calls[0].args.is_empty());
}

#[test]
fn a_button_preview_that_handles_its_press_still_logs_it() {
    let ctx = tap(&button_preview("Counting presses"));
    assert_eq!(ctx.signal("presses", 0u32).get(), 1);
    assert_eq!(ctx.actions().count_of("on_press"), 1);
}
