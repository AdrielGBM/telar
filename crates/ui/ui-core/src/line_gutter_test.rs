use super::*;
use crate::context::{compute_layout, new_container, reset_layout_runtime};
use layout_core::AvailableSpace;
use reactive_core::{RwSignal, signal};
use renderer_core::Color;

fn gutter(count: RwSignal<usize>) -> LineGutter {
    LineGutter::new(
        move || count.get(),
        LayoutStyle::new(),
        || TextStyle::new(14.0, Color::BLACK),
    )
    .unwrap()
}

#[test]
fn height_tracks_line_count() {
    reset_layout_runtime();
    let count = signal(3usize);
    let g = gutter(count);
    let rect = g.leaf.rect;
    let root = new_container(
        LayoutStyle::new().flex_column().width(200.0),
        &[g.leaf.node],
    )
    .unwrap();
    let line_h = crate::text_metrics::line_height(14.0);
    compute_layout(
        root,
        AvailableSpace::Definite(200.0),
        AvailableSpace::MaxContent,
    )
    .unwrap();
    assert!(
        (rect.get().height - 3.0 * line_h).abs() < 0.5,
        "3 lines: {:?}",
        rect.get()
    );

    count.set(10);
    compute_layout(
        root,
        AvailableSpace::Definite(200.0),
        AvailableSpace::MaxContent,
    )
    .unwrap();
    assert!(
        (rect.get().height - 10.0 * line_h).abs() < 0.5,
        "grew to 10 lines: {:?}",
        rect.get()
    );
    assert!(
        rect.get().width > 0.0,
        "gutter reserves a width: {:?}",
        rect.get()
    );
}
