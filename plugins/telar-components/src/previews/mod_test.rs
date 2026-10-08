use std::collections::HashSet;

use telar::preview::host::Args;
use telar::preview::{PreviewCtx, PreviewEntry};
use telar::{AvailableSpace, compute_layout};

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

#[test]
fn every_preview_describes_its_component_by_its_props() {
    for entry in previews() {
        let schema = entry
            .props
            .unwrap_or_else(|| panic!("`{}` names no props", entry.id))();
        assert!(!schema.fields.is_empty(), "`{}`", entry.id);
    }
}
