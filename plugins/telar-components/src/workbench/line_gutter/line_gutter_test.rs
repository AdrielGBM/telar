use super::*;
use crate::test_support::fresh_layout_runtime;
use telar::{
    AvailableSpace, Color, RwSignal, TextArea, compute_layout, new_container, signal, track_layout,
};

fn gutter(count: RwSignal<usize>) -> LineGutter {
    LineGutter::new(
        move || count.get(),
        LayoutStyle::new(),
        || TextStyle::new(14.0, Color::BLACK),
    )
    .unwrap()
}

fn lay_out(node: NodeId) {
    let root = new_container(LayoutStyle::new().flex_column().width(200.0), &[node]).unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(200.0),
        AvailableSpace::MaxContent,
    )
    .unwrap();
}

#[test]
fn height_tracks_line_count() {
    fresh_layout_runtime();
    let count = signal(3usize);
    let g = gutter(count);
    let rect = g.leaf.rect;
    let root = new_container(
        LayoutStyle::new().flex_column().width(200.0),
        &[g.leaf.node],
    )
    .unwrap();
    let line_h = line_box(&TextStyle::new(14.0, Color::BLACK));
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

#[test]
fn a_restyled_gutter_is_measured_again() {
    fresh_layout_runtime();
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
    fresh_layout_runtime();
    let style = || TextStyle::new(14.0, Color::BLACK).with_line_height(1.0);
    let g = LineGutter::new(|| 4, LayoutStyle::new(), style).unwrap();
    let area = TextArea::new(signal("a\nb\nc\nd".to_string()), LayoutStyle::new(), style).unwrap();
    lay_out(g.leaf.node);
    let gutter_height = g.leaf.rect.get().height;
    lay_out(area.layout_node());
    let area_height = track_layout(area.layout_node()).unwrap().get().height;
    assert!(
        (gutter_height - 4.0 * 14.0).abs() < 0.5,
        "gutter={gutter_height}"
    );
    assert!(
        (gutter_height - area_height).abs() < 0.5,
        "gutter={gutter_height} area={area_height}"
    );
}

#[test]
fn an_excerpt_is_numbered_from_its_first_line_and_measured_by_its_last() {
    fresh_layout_runtime();
    let first = signal(1usize);
    let style = || TextStyle::new(14.0, Color::BLACK);
    let g = LineGutter::starting_at(move || first.get(), || 3, LayoutStyle::new(), style).unwrap();
    let root = new_container(
        LayoutStyle::new()
            .flex_column()
            .width(200.0)
            .align_items(telar::AlignItems::START),
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
        g.leaf.rect.get().width
    };
    let narrow = lay();
    first.set(998);
    let wide = lay();
    assert!(wide > narrow * 2.0, "narrow={narrow} wide={wide}");
    let drawn = telar::testing::texts(&telar::ComponentList::new(g));
    assert_eq!(drawn, ["998\n999\n1000"]);
}
