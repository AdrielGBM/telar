use std::cell::Cell;
use std::time::Duration;

use telar::testing::{centre, moved, press, release, route};
use telar::{
    AvailableSpace, ComponentList, DrawCommand, PathData, WindowCommand, compute_layout,
    relayout_if_dirty, set_surface_maximized, take_window_commands,
};

use super::*;

const BAR: (f32, f32) = (400.0, 40.0);

fn laid_out(item: Box<dyn LayoutItem>, (width, height): (f32, f32)) -> ComponentList {
    compute_layout(
        item.layout_node(),
        AvailableSpace::Definite(width),
        AvailableSpace::Definite(height),
    )
    .unwrap();
    relayout_if_dirty();
    ComponentList::new(item)
}

fn controls(props: WindowControlsProps) -> telar::testing::Scoped<ComponentList> {
    crate::test_support::fresh_layout_runtime();
    set_surface_maximized(false);
    take_window_commands();
    telar::testing::scoped(|| laid_out(window_controls(props, Children::default()).unwrap(), BAR))
}

fn nodes(tree: &ComponentList) -> Vec<telar::AccessNode> {
    ui_core::accessibility::snapshot(&tree.commands())
}

/// What a reader hears each button called and where it is drawn, in order.
fn buttons(tree: &ComponentList) -> Vec<(String, telar::Rect)> {
    nodes(tree)
        .into_iter()
        .filter(|node| node.role == Role::Button)
        .map(|node| (node.name, node.rect))
        .collect()
}

fn names(tree: &ComponentList) -> Vec<String> {
    buttons(tree).into_iter().map(|(name, _)| name).collect()
}

fn button(tree: &ComponentList, name: &str) -> telar::Rect {
    buttons(tree)
        .into_iter()
        .find(|(named, _)| named == name)
        .map(|(_, rect)| rect)
        .unwrap_or_else(|| panic!("no button named {name}"))
}

fn click(tree: &mut ComponentList, name: &str) {
    let (x, y) = centre(button(tree, name));
    route(tree, &press(x, y));
    route(tree, &release(x, y));
}

fn asked() -> Vec<WindowCommand> {
    take_window_commands()
        .into_iter()
        .filter(|command| !matches!(command, WindowCommand::SetCursor(_)))
        .collect()
}

fn marks(tree: &ComponentList) -> Vec<PathData> {
    tree.commands()
        .iter()
        .filter_map(|command| match command {
            DrawCommand::Path { data, .. } => Some((**data).clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn the_controls_are_minimize_maximize_and_close_in_a_group_named_window() {
    let tree = controls(WindowControlsProps::props().build());
    assert_eq!(names(&tree), ["Minimize", "Maximize", "Close"]);
    let reading = ui_core::accessibility::FrameReading::of(&tree.commands());
    assert!(
        reading
            .boxes()
            .any(|(_, read)| read.name.as_deref() == Some("Window")),
        "the buttons are not grouped as the window's"
    );
}

#[test]
fn minimize_and_maximize_can_be_left_out_and_close_cannot() {
    let tree = controls(
        WindowControlsProps::props()
            .minimize(false)
            .maximize(false)
            .build(),
    );
    assert_eq!(names(&tree), ["Close"]);
}

#[test]
fn a_maximized_window_is_offered_restore_drawn_as_two_squares() {
    let tree = controls(WindowControlsProps::props().build());
    let at_rest = marks(&tree);
    set_surface_maximized(true);
    relayout_if_dirty();
    assert_eq!(names(&tree), ["Minimize", "Restore", "Close"]);
    let maximized = marks(&tree);
    assert_eq!(maximized[0], at_rest[0], "minimize keeps its mark");
    assert_ne!(maximized[1], at_rest[1], "maximize keeps its square");
    assert_eq!(maximized[2], at_rest[2], "close keeps its mark");

    set_surface_maximized(false);
    relayout_if_dirty();
    assert_eq!(names(&tree), ["Minimize", "Maximize", "Close"]);
    assert_eq!(marks(&tree), at_rest);
}

#[test]
fn minimize_and_maximize_ask_the_platform() {
    let mut tree = controls(WindowControlsProps::props().build());
    click(&mut tree, "Minimize");
    assert_eq!(asked(), [WindowCommand::Minimize]);
    click(&mut tree, "Maximize");
    assert_eq!(asked(), [WindowCommand::ToggleMaximize]);
}

#[test]
fn close_closes_the_window_unless_told_otherwise() {
    let mut tree = controls(WindowControlsProps::props().build());
    click(&mut tree, "Close");
    assert_eq!(asked(), [WindowCommand::Close]);
    drop(tree);

    let closed = Rc::new(Cell::new(0));
    let count = closed.clone();
    let mut tree = controls(
        WindowControlsProps::props()
            .on_close(Rc::new(move || count.set(count.get() + 1)))
            .build(),
    );
    click(&mut tree, "Close");
    assert_eq!(closed.get(), 1);
    assert!(
        asked().is_empty(),
        "a close of the caller's own closes nothing else"
    );
}

#[test]
fn every_button_is_at_least_the_size_asked_for() {
    let tree = controls(WindowControlsProps::props().build());
    for (name, rect) in buttons(&tree) {
        assert!(rect.width >= shared::MIN_TARGET, "{name} is {rect:?}");
        assert!(rect.height >= shared::MIN_TARGET, "{name} is {rect:?}");
    }
    drop(tree);

    let tree = controls(WindowControlsProps::props().min_size(32.0).build());
    for (name, rect) in buttons(&tree) {
        assert_eq!(rect.height, 32.0, "{name} is {rect:?}");
        assert!(rect.width >= 32.0, "{name} is {rect:?}");
    }
}

/// A bar of `width` with nothing in it, made a title bar as `options` say.
fn plain_bar() -> StyledContainer {
    crate::test_support::fresh_layout_runtime();
    take_window_commands();
    StyledContainer::new(
        LayoutStyle::new().width(BAR.0).height(BAR.1),
        |_| RectStyle::default(),
        vec![],
    )
    .unwrap()
}

fn bar(options: TitleBarOptions) -> ComponentList {
    laid_out(
        box_item(title_bar(plain_bar().role(Role::Banner), options)),
        BAR,
    )
}

fn click_at(tree: &mut ComponentList, (x, y): (f64, f64)) {
    route(tree, &press(x, y));
    route(tree, &release(x, y));
}

const MIDDLE: (f64, f64) = (BAR.0 as f64 / 2.0, BAR.1 as f64 / 2.0);

#[test]
fn a_drag_on_the_bar_moves_the_window_once_it_has_travelled() {
    let mut tree = bar(TitleBarOptions::default());
    let (x, y) = MIDDLE;
    route(&mut tree, &press(x, y));
    assert!(asked().is_empty(), "a press alone moves nothing");
    route(&mut tree, &moved(x + 20.0, y));
    assert_eq!(asked(), [WindowCommand::Drag]);
}

#[test]
fn a_double_click_on_the_bar_maximizes_the_window() {
    let mut tree = bar(TitleBarOptions::default());
    click_at(&mut tree, MIDDLE);
    assert!(asked().is_empty(), "one click does nothing");
    click_at(&mut tree, MIDDLE);
    assert_eq!(asked(), [WindowCommand::ToggleMaximize]);
    std::thread::sleep(DOUBLE_CLICK + Duration::from_millis(50));
    click_at(&mut tree, MIDDLE);
    assert!(
        asked().is_empty(),
        "a click long after is a first click again"
    );
}

#[test]
fn a_bar_of_a_window_that_offers_no_maximize_does_not_maximize_it() {
    let mut tree = bar(TitleBarOptions {
        maximize: false,
        ..TitleBarOptions::default()
    });
    click_at(&mut tree, MIDDLE);
    click_at(&mut tree, MIDDLE);
    assert!(asked().is_empty());
}

#[test]
fn a_bar_that_does_not_drag_leaves_the_window_where_it_is() {
    let mut tree = bar(TitleBarOptions {
        drag: false,
        maximize: false,
        ..TitleBarOptions::default()
    });
    let (x, y) = MIDDLE;
    route(&mut tree, &press(x, y));
    route(&mut tree, &moved(x + 20.0, y));
    assert!(asked().is_empty());
}

#[test]
fn a_drag_from_a_window_control_in_the_bar_does_not_move_the_window() {
    crate::test_support::fresh_layout_runtime();
    take_window_commands();
    let controls =
        window_controls(WindowControlsProps::props().build(), Children::default()).unwrap();
    let bar = StyledContainer::new(
        LayoutStyle::new().width(BAR.0).height(BAR.1),
        |_| RectStyle::default(),
        vec![controls],
    )
    .unwrap()
    .role(Role::Banner);
    let mut tree = laid_out(box_item(title_bar(bar, TitleBarOptions::default())), BAR);
    let (x, y) = centre(button(&tree, "Minimize"));
    route(&mut tree, &press(x, y));
    route(&mut tree, &moved(x + 20.0, y));
    route(&mut tree, &release(x + 20.0, y));
    assert!(!asked().contains(&WindowCommand::Drag));
}

#[test]
fn a_bar_that_replaces_the_decorations_gives_them_back_when_it_goes() {
    let tree = telar::testing::scoped(|| {
        bar(TitleBarOptions {
            replace_decorations: true,
            ..TitleBarOptions::default()
        })
    });
    assert_eq!(asked(), [WindowCommand::SetDecorations(false)]);
    drop(tree);
    assert_eq!(asked(), [WindowCommand::SetDecorations(true)]);
}

#[test]
fn a_bar_that_leaves_the_decorations_alone_never_touches_them() {
    drop(telar::testing::scoped(|| bar(TitleBarOptions::default())));
    assert!(
        !take_window_commands()
            .iter()
            .any(|command| matches!(command, WindowCommand::SetDecorations(_)))
    );
}

#[test]
fn a_bar_without_a_role_becomes_a_group() {
    let bar = title_bar(plain_bar(), TitleBarOptions::default());
    assert_eq!(bar.declared_role(), Some(Role::Group));
}

#[test]
fn a_bar_with_a_role_keeps_it() {
    let bar = title_bar(plain_bar().role(Role::Banner), TitleBarOptions::default());
    assert_eq!(bar.declared_role(), Some(Role::Banner));
}
