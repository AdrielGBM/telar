use layout_core::AvailableSpace;
use renderer_core::{Color, DrawCommand};
use ui_core::{ComponentList, compute_layout, new_container};

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
        Children::default(),
    )
    .unwrap();
    let root = new_container(
        LayoutStyle::new().flex_column().width(400.0).height(100.0),
        &[row.layout_node()],
    )
    .unwrap();
    let declared = Color::rgba(0.9, 0.2, 0.1, 1.0);
    ui_core::declare(
        root,
        renderer_core::Declared::default().with_color(declared),
    );
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
