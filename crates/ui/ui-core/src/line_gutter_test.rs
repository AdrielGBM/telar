use super::*;
use crate::context::{compute_layout, new_container, reset_layout_runtime};
use crate::layout_item::LayoutItem;
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

fn laid_height(g: &LineGutter) -> f32 {
    let root = new_container(
        LayoutStyle::new().flex_column().width(200.0),
        &[g.leaf.node],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(200.0),
        AvailableSpace::MaxContent,
    )
    .unwrap();
    g.leaf.rect.get().height
}

#[test]
fn a_restyled_gutter_is_measured_again() {
    reset_layout_runtime();
    let size = signal(12.0f32);
    let g = LineGutter::new(
        || 3,
        LayoutStyle::new(),
        move || TextStyle::new(size.get(), Color::BLACK),
    )
    .unwrap();
    let root = new_container(
        LayoutStyle::new().flex_column().width(200.0),
        &[g.leaf.node],
    )
    .unwrap();
    let lay = || {
        compute_layout(
            root,
            AvailableSpace::Definite(200.0),
            AvailableSpace::MaxContent,
        )
        .unwrap();
        g.leaf.rect.get().height
    };
    let small = lay();
    size.set(48.0);
    let large = lay();
    assert!(large > small * 3.5, "small={small} large={large}");
}

#[test]
fn gutter_and_area_agree_under_a_declared_line_height() {
    reset_layout_runtime();
    let style = || TextStyle::new(14.0, Color::BLACK).with_line_height(1.0);
    let g = LineGutter::new(|| 4, LayoutStyle::new(), style).unwrap();
    let area =
        crate::TextArea::new(signal("a\nb\nc\nd".to_string()), LayoutStyle::new(), style).unwrap();
    let gutter_height = laid_height(&g);
    let root = new_container(
        LayoutStyle::new().flex_column().width(200.0),
        &[area.layout_node()],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(200.0),
        AvailableSpace::MaxContent,
    )
    .unwrap();
    let area_height = crate::context::track_layout(area.layout_node())
        .unwrap()
        .get()
        .height;
    assert!(
        (gutter_height - 4.0 * 14.0).abs() < 0.5,
        "gutter={gutter_height}"
    );
    assert!(
        (gutter_height - area_height).abs() < 0.5,
        "gutter={gutter_height} area={area_height}"
    );
}
