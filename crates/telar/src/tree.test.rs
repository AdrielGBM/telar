use super::*;

/// A button inside a box, its focus id handed out as the tree is built.
pub(super) fn panel_with_a_button(
    id: std::rc::Rc<std::cell::Cell<Option<ui_core::focus::FocusId>>>,
) -> Box<dyn Component> {
    let button = ui_core::StyledContainer::new(
        layout_core::LayoutStyle::new().width(40.0).height(20.0),
        |_| renderer_core::RectStyle::default(),
        vec![],
    )
    .expect("a box builds")
    .control(ui_core::focus::Role::Button)
    .on_press(|| {});
    id.set(button.focus_id());
    Box::new(
        ui_core::Container::new(
            layout_core::LayoutStyle::new(),
            vec![ui_core::box_item(button)],
        )
        .expect("a box builds"),
    )
}

/// What a mounted tree built is the tree's: dropping it — a remount, a window closing — leaves no control of it in the tab order, however deep it sat. A tree that only held its root left every nested control registered and every effect of the old tree running.
#[test]
fn dropping_a_mounted_tree_frees_the_controls_it_built() {
    ui_core::reset_layout_runtime();
    let id = std::rc::Rc::new(std::cell::Cell::new(None));
    let tree = LocalTree::new({
        let id = id.clone();
        move || panel_with_a_button(id)
    });
    let button = id.get().expect("the button is focusable");
    assert!(ui_core::focus::is_registered(button));

    drop(tree);
    assert!(
        !ui_core::focus::is_registered(button),
        "the button left the tab order with its tree"
    );
}

fn fill() -> layout_core::LayoutStyle {
    layout_core::LayoutStyle::new()
        .width(layout_core::SizeDimension::Percent(1.0))
        .height(layout_core::SizeDimension::Percent(1.0))
}

fn swatch() -> Box<dyn ui_core::LayoutItem> {
    Box::new(
        ui_core::Rectangle::new(fill(), || {
            renderer_core::RectStyle::filled(renderer_core::Color::BLACK, 0.0)
        })
        .expect("a rectangle builds"),
    )
}

fn resize(tree: &mut dyn UiTree) {
    tree.on_event(&Event::WindowResized {
        width: 400,
        height: 300,
    });
    ui_core::relayout_if_dirty();
}

/// The runner drops a tree before it builds the next. One that took only its owner with it left its nodes behind and its root the overlay host, so the next tree's overlay portaled onto a dead root and the next layout pass read rect signals its owner had freed — whether or not the first tree had an overlay of its own.
#[test]
fn a_remounted_tree_portals_onto_its_own_root_and_leaves_no_node_of_the_last_one() {
    ui_core::reset_layout_runtime();
    let baseline = ui_core::live_node_count();
    let mut first = LocalTree::new(|| {
        Box::new(ui_core::WindowRoot::wrapping(swatch()).expect("a root builds"))
    });
    resize(&mut first);
    drop(first);
    assert_eq!(
        ui_core::live_node_count(),
        baseline,
        "the first tree took its nodes with it"
    );

    let content = std::rc::Rc::new(std::cell::Cell::new(None));
    let mut second = LocalTree::new({
        let content = content.clone();
        move || {
            let overlay =
                ui_core::overlay::Overlay::new(fill(), vec![swatch()]).expect("an overlay builds");
            content.set(Some(overlay.content_node()));
            let page =
                ui_core::Container::new(fill(), vec![Box::new(overlay)]).expect("a page builds");
            Box::new(ui_core::WindowRoot::wrapping(Box::new(page)).expect("a root builds"))
        }
    });
    resize(&mut second);
    let content = content.get().expect("the overlay was built");
    assert_eq!(
        ui_core::track_layout(content).map(|rect| rect.peek()),
        Some(geometry_core::Rect::new(0.0, 0.0, 400.0, 300.0)),
        "the overlay fills the new tree's window"
    );

    drop(second);
    assert_eq!(
        ui_core::live_node_count(),
        baseline,
        "and so did the second"
    );
}
