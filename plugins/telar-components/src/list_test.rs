use telar::{AvailableSpace, Color, ComponentList, DrawCommand, compute_layout, new_container};

use super::*;

/// A row's hint is quieter than the label beside it — which means quieter than whatever that label is written in, not a fixed fraction of the theme's own ink. A menu in a region that declared its colour used to get a hint in the theme's near-black however light the rest of the region was.
#[test]
fn a_hint_fades_the_ink_of_the_row_it_trails() {
    crate::test_support::fresh_layout_runtime();
    let row = item(
        ItemProps::props()
            .label("Move")
            .hint(Reactive::from("⌘G"))
            .build(),
    )
    .unwrap();
    let root = new_container(
        LayoutStyle::new().flex_column().width(400.0).height(100.0),
        &[row.layout_node()],
    )
    .unwrap();
    let declared = Color::rgba(0.9, 0.2, 0.1, 1.0);
    telar::declare(root, telar::Declared::default().with_color(declared));
    compute_layout(
        root,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();

    let tree = ComponentList::new(row);
    let ink = tree
        .commands()
        .iter()
        .find_map(|c| match c {
            DrawCommand::Text { text, style, .. } if text.as_ref() == "⌘G" => {
                Some(style.color.solid_color())
            }
            _ => None,
        })
        .expect("the row drew its hint");
    assert_eq!(ink, declared.with_alpha(shared::QUIET_ALPHA));
}

/// A row of every list is the shared row height at the control size in force: smaller in a compact region, larger in a roomy one, and the same number every other list's rows are.
#[test]
fn a_row_stands_at_the_shared_row_height_at_every_control_size() {
    use telar::{ControlSize, provide_control_size, track_layout};

    let height_at = |size: ControlSize| {
        crate::test_support::fresh_layout_runtime();
        let mut wanted = 0.0;
        let row = provide_control_size(size, || {
            wanted = ListTokens::current().row_height();
            item(ItemProps::props().label("Move").build())
        })
        .unwrap();
        let rect = track_layout(row.layout_node()).unwrap();
        compute_layout(
            row.layout_node(),
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();
        let _tree = ComponentList::new(row);
        (rect.get().height, wanted)
    };
    let sizes = [
        ControlSize::Mini,
        ControlSize::Small,
        ControlSize::Regular,
        ControlSize::Large,
    ];
    let heights: Vec<(f32, f32)> = sizes.into_iter().map(height_at).collect();
    for (height, wanted) in &heights {
        assert_eq!(height, wanted, "{heights:?}");
    }
    assert!(
        heights.windows(2).all(|pair| pair[0].0 < pair[1].0),
        "rows grow with the control size: {heights:?}"
    );
}
