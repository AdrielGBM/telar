use telar::testing::{named, press, release, route};
use telar::{
    AvailableSpace, Component, ComponentList, Direction, DrawCommand, NamedKey, NodeId,
    compute_layout, new_container,
};

use super::*;

/// A bar in a region that declared its own ink writes the tabs that are not selected in a quieter shade of *that*, rather than in a grey no theme and no declaration can move.
#[test]
fn an_inactive_tab_fades_the_ink_of_the_region_around_it() {
    crate::test_support::fresh_layout_runtime();
    let item = tabs(
        TabsProps::props().items(vec!["One", "Two"]).build(),
        Children::default(),
    )
    .unwrap();
    let root = new_container(
        LayoutStyle::new().flex_column().width(400.0).height(100.0),
        &[item.layout_node()],
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

    let tree = ComponentList::new(item);
    let ink = tree
        .commands()
        .iter()
        .find_map(|c| match c {
            DrawCommand::Text { text, style, .. } if text.as_ref() == "Two" => {
                Some(style.color.solid_color())
            }
            _ => None,
        })
        .expect("the bar drew the tab that is not selected");
    assert_eq!(ink, declared.with_alpha(shared::QUIET_ALPHA));
}

// As the sole child of a 400×100 root: an auto-size root fills its available space, so laying `node` out as the root would force it to 400px wide instead of its natural content width.
fn lay_out(node: NodeId) -> telar::Rect {
    telar::testing::lay_out_row(node, 400.0, 100.0)
}

#[test]
fn builds_and_lays_out() {
    crate::test_support::fresh_layout_runtime();
    let item = tabs(
        TabsProps::props()
            .items(vec!["One", "Two", "Three"])
            .build(),
        Children::default(),
    )
    .unwrap();
    let r = lay_out(item.layout_node());
    assert!(r.width > 0.0, "the tab row takes some width");
    let _ = item.view();
}

// With two content-sized tabs and no `flex_grow`, the row's right edge sits flush against the second tab's padding, so a point just inside it lands on index 1 without hardcoding font metrics.
#[test]
fn pressing_the_last_tab_sets_selected_index() {
    crate::test_support::fresh_layout_runtime();
    let selected = signal(0u32);
    let mut item = tabs(
        TabsProps::props()
            .items(vec!["One", "Two"])
            .selected(selected)
            .build(),
        Children::default(),
    )
    .unwrap();
    let r = lay_out(item.layout_node());
    let (cx, cy) = ((r.x + r.width - 2.0) as f64, (r.y + r.height / 2.0) as f64);

    item.on_event(&press(cx, cy));
    item.on_event(&release(cx, cy));

    assert_eq!(
        selected.get(),
        1,
        "pressing the last (second) tab sets selected to its index"
    );
}

/// The bar is the tab list its tabs belong to, named when the caller names it, and naming it does not rename a tab.
#[test]
fn the_bar_is_a_tab_list_a_reader_can_name() {
    crate::test_support::fresh_layout_runtime();
    telar::focus::clear();
    let item = tabs(
        TabsProps::props()
            .items(vec!["General", "Advanced"])
            .label("Settings sections")
            .build(),
        Children::default(),
    )
    .unwrap();
    lay_out(item.layout_node());
    let tree = ComponentList::new(item);
    let nodes = ui_core::accessibility::snapshot(&tree.commands());

    let mut tabs: Vec<_> = nodes
        .iter()
        .filter(|node| node.role == Role::Tab)
        .map(|node| node.name.as_str())
        .collect();
    tabs.sort();
    assert_eq!(tabs, ["Advanced", "General"]);
    assert!(
        nodes
            .iter()
            .any(|node| node.id.is_none() && node.name == "Settings sections"),
        "the list's name is read: {nodes:?}"
    );
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Page {
    First,
    Second,
    Third,
}

fn pages() -> Vec<Tab<Page>> {
    vec![
        Tab::new(Page::First, "First"),
        Tab::new(Page::Second, "Second"),
        Tab::new(Page::Third, "Third"),
    ]
}

/// A row of three tabs with the first one pressed, so it holds focus and the keys that follow are its to answer.
fn focused_row(selected: RwSignal<Page>) -> ComponentList {
    crate::test_support::fresh_layout_runtime();
    telar::focus::clear();
    let item = tab_list(selected, pages(), Reactive::of(|| Color::TRANSPARENT), None).unwrap();
    let r = lay_out(item.layout_node());
    let mut tree = ComponentList::new(item);
    let (x, y) = ((r.x + 2.0) as f64, (r.y + r.height / 2.0) as f64);
    route(&mut tree, &press(x, y));
    route(&mut tree, &release(x, y));
    tree
}

#[test]
fn a_tab_list_selects_a_value_that_is_not_an_index() {
    let selected = signal(Page::Third);
    let mut tree = focused_row(selected);
    assert_eq!(selected.get(), Page::First, "the first tab was pressed");
    route(&mut tree, &named(NamedKey::ArrowRight));
    assert_eq!(selected.get(), Page::Second);
}

#[test]
fn the_arrows_home_and_end_move_the_selection_along_the_row_and_wrap() {
    let selected = signal(Page::First);
    let mut tree = focused_row(selected);
    route(&mut tree, &named(NamedKey::ArrowLeft));
    assert_eq!(selected.get(), Page::Third, "wraps back past the start");
    route(&mut tree, &named(NamedKey::ArrowRight));
    assert_eq!(selected.get(), Page::First, "and forward past the end");
    route(&mut tree, &named(NamedKey::End));
    assert_eq!(selected.get(), Page::Third);
    route(&mut tree, &named(NamedKey::Home));
    assert_eq!(selected.get(), Page::First);
    route(&mut tree, &named(NamedKey::ArrowDown));
    assert_eq!(selected.get(), Page::First, "down is not along the row");
}

#[test]
fn focus_follows_the_selection_so_the_next_key_continues_from_there() {
    let selected = signal(Page::First);
    let mut tree = focused_row(selected);
    route(&mut tree, &named(NamedKey::ArrowRight));
    route(&mut tree, &named(NamedKey::ArrowRight));
    assert_eq!(selected.get(), Page::Third);
}

#[test]
fn the_left_arrow_moves_forward_when_the_surface_reads_right_to_left() {
    let selected = signal(Page::First);
    let mut tree = focused_row(selected);
    telar::set_direction(Direction::Rtl);
    route(&mut tree, &named(NamedKey::ArrowLeft));
    telar::set_direction(Direction::Ltr);
    assert_eq!(selected.get(), Page::Second);
}

#[test]
fn a_trailing_item_is_drawn_but_not_part_of_the_tabs_name() {
    crate::test_support::fresh_layout_runtime();
    telar::focus::clear();
    let badge = Text::declaring(|| "7".to_string(), LayoutStyle::new(), |t| t).unwrap();
    let selected = signal(Page::First);
    let item = tab_list(
        selected,
        vec![Tab::new(Page::First, "Actions").trailing(box_item(badge))],
        Reactive::of(|| Color::TRANSPARENT),
        None,
    )
    .unwrap();
    lay_out(item.layout_node());
    let tree = ComponentList::new(item);
    let nodes = ui_core::accessibility::snapshot(&tree.commands());
    assert!(
        nodes
            .iter()
            .any(|node| node.role == Role::Tab && node.name == "Actions")
    );
    assert!(telar::testing::find_text(&tree, "7"));
}

#[test]
fn a_reactive_label_follows_what_it_reads() {
    crate::test_support::fresh_layout_runtime();
    telar::focus::clear();
    let word = signal("Canvas".to_string());
    let item = tab_list(
        signal(Page::First),
        vec![Tab::new(Page::First, word)],
        Reactive::of(|| Color::TRANSPARENT),
        None,
    )
    .unwrap();
    let node = item.layout_node();
    lay_out(node);
    let tree = ComponentList::new(item);
    assert!(telar::testing::find_text(&tree, "Canvas"));
    word.set("Lienzo".to_string());
    lay_out(node);
    assert!(telar::testing::find_text(&tree, "Lienzo"));
}

#[cfg(feature = "workbench")]
#[test]
fn an_icon_tab_is_the_same_square_as_an_icon_button() {
    crate::test_support::fresh_layout_runtime();
    let tab_list = tab_list(
        signal(0u32),
        vec![Tab::new(0u32, "One").icon(telar_icons::icon!("lucide:square"))],
        Color::TRANSPARENT.into(),
        None,
    )
    .unwrap();
    let button = crate::icon_button(
        crate::IconButtonProps::props()
            .icon(telar_icons::icon!("lucide:square"))
            .label("Two")
            .build(),
        Children::default(),
    )
    .unwrap();
    let root = telar::Container::new(
        LayoutStyle::new().flex_row().width(400.0).height(100.0),
        vec![tab_list, button],
    )
    .unwrap();
    compute_layout(
        root.layout_node(),
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();
    let _tree = ComponentList::new(telar::box_item(root));
    let tab = crate::test_support::drawn(Role::Tab)[0];
    let pressed = crate::test_support::drawn(Role::Button)[0];
    assert_eq!((tab.width, tab.height), (pressed.width, pressed.height));
}

fn mounted(props: TabsProps) -> ComponentList {
    crate::test_support::fresh_layout_runtime();
    let item = tabs(props, Children::default()).unwrap();
    lay_out(item.layout_node());
    ComponentList::new(item)
}

fn tab_at(tree: &ComponentList, index: usize) -> RectStyle {
    crate::test_support::painted_at(tree, crate::test_support::drawn(Role::Tab)[index])
}

#[test]
fn the_corner_is_the_theme_s_control_role_until_the_tabs_tokens_name_one() {
    crate::test_support::assert_corner_follows_the_role_until_the_tokens_name_one(
        || mounted(TabsProps::props().items(vec!["One", "Two"]).build()),
        |tree| tab_at(tree, 0).radius,
        |_, role| BorderRadius::all(role),
        |radius| TabsTokens {
            radius: Some(radius),
            ..TabsTokens::default()
        },
    );
}

/// `@class` and the `style` prop amend the finished paint of every tab, so they reach the selected tab, the others, and a tab under the pointer.
#[test]
fn a_style_amendment_reshapes_every_tab_in_every_state() {
    use telar::testing::{centre, moved};

    let flat: SurfaceAmendment = Rc::new(|s: RectStyle| s.with_radius(BorderRadius::all(0.0)));
    let mut tree = mounted(
        TabsProps::props()
            .items(vec!["One", "Two"])
            .style(flat)
            .build(),
    );
    let selected = tab_at(&tree, 0);
    assert!(selected.fill.is_some(), "the selected tab is filled");
    assert_eq!(selected.radius, BorderRadius::all(0.0));
    assert_eq!(tab_at(&tree, 1).radius, BorderRadius::all(0.0));
    let (x, y) = centre(crate::test_support::drawn(Role::Tab)[1]);
    route(&mut tree, &moved(x, y));
    let hovered = tab_at(&tree, 1);
    assert_eq!(
        hovered.fill.map(|paint| paint.solid_color()),
        Some(telar::use_theme_tokens().highlight_low()),
        "the hover wash is drawn"
    );
    assert_eq!(
        hovered.radius,
        BorderRadius::all(0.0),
        "on the amended shape"
    );
}

#[test]
fn the_padding_tokens_scale_with_the_control_size() {
    crate::test_support::fresh_layout_runtime();
    let tokens = TabsTokens {
        pad_x: Some(10.0),
        ..TabsTokens::default()
    };
    let mut padded = 0.0;
    let _ = telar::provide_control_size(telar::ControlSize::Large, || {
        padded = tokens.pad_x();
        telar::Container::new(LayoutStyle::new(), vec![]).map(box_item)
    });
    assert_eq!(padded, 12.5);
}

/// A count after a tab's name reads as its own word, not run into the name.
#[test]
fn a_trailing_item_keeps_a_gap_from_the_name() {
    crate::test_support::fresh_layout_runtime();
    telar::focus::clear();
    let badge = Text::declaring(|| "7".to_string(), LayoutStyle::new(), |t| t).unwrap();
    let item = tab_list(
        signal(Page::First),
        vec![Tab::new(Page::First, "Actions").trailing(box_item(badge))],
        Reactive::of(|| Color::TRANSPARENT),
        None,
    )
    .unwrap();
    lay_out(item.layout_node());
    let tree = ComponentList::new(item);
    let at = |wanted: &str| {
        let mut found = None;
        telar::for_each_with_matrix(&tree.commands(), |command, matrix| {
            if let DrawCommand::Text { text, rect, .. } = command
                && text.as_ref() == wanted
            {
                found = Some(telar::transform_clip_rect(matrix, *rect));
            }
        });
        found.unwrap_or_else(|| panic!("{wanted} is drawn"))
    };
    let (name, count) = (at("Actions"), at("7"));
    let gap = count.x - (name.x + name.width);
    assert!((gap - shared::gap_sm()).abs() < 0.5, "{gap}");
}
