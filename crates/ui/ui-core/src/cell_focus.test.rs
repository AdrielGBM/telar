//! The focus ring on a surface drawn in whole cells, in a process of its own because the layout grid is process-wide.

use geometry_core::{LayoutGrid, Size, set_layout_grid};
use layout_core::LayoutStyle;
use platform_core::{Event, Location};
use renderer_core::{DrawCommand, Paint, RectStyle};
use telar_ui_core::{ComponentList, StyledContainer, WindowRoot, box_item, focus};

fn fills(tree: &ComponentList) -> Vec<Option<Paint>> {
    tree.commands()
        .iter()
        .filter_map(|command| match command {
            DrawCommand::Rect { style, .. } => Some(style.fill),
            _ => None,
        })
        .collect()
}

/// A link whose words fill its box has no cell left for a frame, since the words are drawn over it. The ring's tint under those words is what shows the keyboard is there.
#[test]
fn a_box_with_no_room_for_a_frame_is_tinted_while_it_has_focus() {
    set_layout_grid(LayoutGrid::new(8.0, 16.0));
    telar_ui_core::set_surface_size(Size::new(160.0, 48.0));
    let link = StyledContainer::new(
        LayoutStyle::new().width(48.0).height(16.0),
        |_| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .to(|| Location::root().segment("docs"));
    let mut tree = ComponentList::new(WindowRoot::new(box_item(link)));
    tree.on_event(&Event::WindowResized {
        width: 160,
        height: 48,
    });
    assert!(fills(&tree).iter().all(Option::is_none));

    focus::focus_next();
    assert!(
        fills(&tree).iter().any(Option::is_some),
        "the focused link is tinted"
    );
}
