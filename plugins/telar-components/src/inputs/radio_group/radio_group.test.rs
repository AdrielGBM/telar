use std::cell::RefCell;
use std::rc::Rc;

use telar::testing::{centre, named, press, release};
use telar::{
    AccessNode, BorderRadius, ComponentList, ConsumedKeys, LayoutItem, NamedKey, Toggle, signal,
};

use super::*;

const SIZES: [&str; 3] = ["Small", "Medium", "Large"];

/// The three sizes choosing into `selected` and reporting each choice to `seen`, laid out.
fn group(selected: RwSignal<Option<usize>>, seen: &Rc<RefCell<Vec<usize>>>) -> ComponentList {
    crate::test_support::fresh_layout_runtime();
    telar::focus::clear();
    let sink = seen.clone();
    let item = radio_group(
        RadioGroupProps::props()
            .items(SIZES.to_vec())
            .label("Size")
            .selected(selected)
            .on_select(Rc::new(move |at| sink.borrow_mut().push(at)))
            .build(),
    )
    .unwrap();
    telar::testing::lay_out(item.layout_node(), 300.0, 200.0);
    ComponentList::new(item)
}

fn read(tree: &ComponentList) -> Vec<AccessNode> {
    telar::relayout_if_dirty();
    telar::inspect::snapshot(&tree.commands())
}

fn radio_named<'a>(nodes: &'a [AccessNode], name: &str) -> &'a AccessNode {
    nodes
        .iter()
        .find(|node| node.role == Role::Radio && node.name == name)
        .unwrap_or_else(|| panic!("a radio named {name}: {nodes:?}"))
}

/// The name of the radio holding focus.
fn focused(tree: &ComponentList) -> Option<String> {
    read(tree)
        .into_iter()
        .find(|node| node.role == Role::Radio && node.focused)
        .map(|node| node.name)
}

fn key(tree: &mut ComponentList, key: NamedKey) {
    tree.on_event(&named(key));
}

#[test]
fn the_group_is_a_named_radio_group_holding_its_radios() {
    let tree = group(signal(Some(1)), &Rc::default());
    let nodes = read(&tree);
    let set = nodes
        .iter()
        .position(|node| node.role == Role::RadioGroup && node.name == "Size")
        .unwrap_or_else(|| panic!("a radio group named Size: {nodes:?}"));
    for name in SIZES {
        assert_eq!(radio_named(&nodes, name).parent, Some(set), "{name}");
    }
    assert_eq!(radio_named(&nodes, "Medium").toggled, Some(Toggle::On));
    assert_eq!(radio_named(&nodes, "Small").toggled, Some(Toggle::Off));
}

#[test]
fn a_tap_chooses_the_radio_and_reports_it() {
    let selected = signal(None);
    let seen = Rc::default();
    let mut tree = group(selected, &seen);
    let (x, y) = centre(radio_named(&read(&tree), "Large").rect);
    tree.on_event(&press(x, y));
    tree.on_event(&release(x, y));
    assert_eq!(selected.get(), Some(2));
    assert_eq!(*seen.borrow(), vec![2]);
}

#[test]
fn tab_enters_the_group_once_on_the_chosen_radio_and_leaves_it_on_the_next_press() {
    let tree = group(signal(Some(1)), &Rc::default());
    telar::focus::focus_next();
    assert_eq!(focused(&tree).as_deref(), Some("Medium"));
    let stops = telar::focus::tab_order()
        .into_iter()
        .filter(|stop| stop.role == Role::Radio)
        .count();
    assert_eq!(stops, 1, "the other radios are no stops");
    telar::focus::clear();
}

#[test]
fn with_nothing_chosen_tab_lands_on_the_first_radio() {
    let tree = group(signal(None), &Rc::default());
    telar::focus::focus_next();
    assert_eq!(focused(&tree).as_deref(), Some("Small"));
    telar::focus::clear();
}

#[test]
fn the_arrows_check_and_focus_the_next_radio() {
    let selected = signal(Some(0));
    let seen = Rc::default();
    let mut tree = group(selected, &seen);
    telar::focus::focus_next();

    key(&mut tree, NamedKey::ArrowDown);
    assert_eq!(selected.get(), Some(1));
    assert_eq!(focused(&tree).as_deref(), Some("Medium"));
    key(&mut tree, NamedKey::ArrowRight);
    assert_eq!(focused(&tree).as_deref(), Some("Large"));
    key(&mut tree, NamedKey::ArrowDown);
    assert_eq!(
        (selected.get(), focused(&tree).as_deref()),
        (Some(0), Some("Small")),
        "past the last radio comes the first"
    );
    key(&mut tree, NamedKey::ArrowUp);
    assert_eq!(focused(&tree).as_deref(), Some("Large"));
    key(&mut tree, NamedKey::Home);
    assert_eq!(focused(&tree).as_deref(), Some("Small"));
    assert_eq!(*seen.borrow(), vec![1, 2, 0, 2, 0]);
    telar::focus::clear();
}

/// In a right-to-left surface the arrow pointing the way the text runs, Left, moves on to the next radio.
#[test]
fn left_moves_on_in_a_right_to_left_surface() {
    let selected = signal(Some(0));
    let mut tree = group(selected, &Rc::default());
    telar::set_direction(telar::Direction::Rtl);
    telar::focus::focus_next();
    key(&mut tree, NamedKey::ArrowLeft);
    telar::set_direction(telar::Direction::Ltr);
    assert_eq!(selected.get(), Some(1));
    telar::focus::clear();
}

#[test]
fn every_radio_keeps_both_arrows_the_edges_and_its_activation_keys() {
    let keys =
        crate::test_support::declared_keys(Role::Radio, || group(signal(None), &Rc::default()));
    assert_eq!(
        keys,
        vec![ConsumedKeys::ARROWS | ConsumedKeys::EDGES | ConsumedKeys::ACTIVATION; 3],
    );
}

#[derive(Clone, Default)]
struct Plain {
    radios: Option<RadioTokens>,
}

impl telar::ThemeTokens for Plain {
    fn border_width_strong(&self) -> f32 {
        2.5
    }
    fn register_extensions(&self, extensions: &mut telar::ThemeExtensions) {
        if let Some(radios) = &self.radios {
            extensions.insert(radios.clone());
        }
    }
}

/// The ring and the dot, in paint order.
fn ring_and_dot(item: Box<dyn LayoutItem>) -> Vec<(telar::Rect, RectStyle)> {
    telar::testing::lay_out(item.layout_node(), 200.0, 100.0);
    telar::ComponentList::new(item)
        .commands()
        .iter()
        .filter_map(|command| match command {
            telar::DrawCommand::Rect { rect, style } if style.fill.is_some() => {
                Some((*rect, **style))
            }
            _ => None,
        })
        .collect()
}

/// One radio, chosen, at `size`.
fn chosen(size: ControlSize, style: Option<telar::SurfaceAmendment>) -> Box<dyn LayoutItem> {
    let mut props = RadioGroupProps::props()
        .items(vec!["Small"])
        .selected(signal(Some(0usize)))
        .size(size)
        .build();
    props.style = style;
    radio_group(props).unwrap()
}

#[test]
fn the_ring_and_its_dot_are_pills_framed_at_the_strong_width() {
    crate::test_support::fresh_layout_runtime();
    telar::set_theme(Plain::default());
    let [(ring, ring_style), (dot, dot_style)] =
        ring_and_dot(chosen(ControlSize::Regular, None))[..]
    else {
        panic!("a chosen radio paints its ring and its dot");
    };
    let side = crate::metrics::icon_size(telar::ControlSize::Regular);
    assert_eq!((ring.width, ring.height), (side, side));
    assert_eq!(ring_style.radius, BorderRadius::all(side / 2.0));
    assert_eq!(ring_style.border.map(|b| b.widths[0]), Some(2.5));
    assert_eq!(
        dot.width,
        side - crate::metrics::gap_sm(telar::ControlSize::Regular) * 2.0
    );
    assert_eq!(dot_style.radius, BorderRadius::all(dot.width / 2.0));
}

#[test]
fn the_radio_tokens_win_over_the_roles() {
    crate::test_support::fresh_layout_runtime();
    telar::set_theme(Plain {
        radios: Some(RadioTokens {
            size: Some(24.0),
            border_width: Some(1.0),
        }),
    });
    let (ring, style) = ring_and_dot(chosen(ControlSize::Regular, None))[0];
    assert_eq!((ring.width, ring.height), (24.0, 24.0));
    assert_eq!(style.radius, BorderRadius::all(12.0));
    assert_eq!(style.border.map(|b| b.widths[0]), Some(1.0));
}

/// The ring is the icon size of the group's own size, and the dot stands inside it by that size's step.
#[test]
fn the_ring_and_its_dot_scale_with_the_group_s_size() {
    for size in [ControlSize::Small, ControlSize::Regular, ControlSize::Large] {
        crate::test_support::fresh_layout_runtime();
        let [(ring, _), (dot, _)] = ring_and_dot(chosen(size, None))[..] else {
            panic!("a chosen radio paints its ring and its dot");
        };
        let side = crate::metrics::icon_size(size);
        assert_eq!(ring.width, side, "{size:?}");
        assert_eq!(
            dot.width,
            side - crate::metrics::gap_sm(size) * 2.0,
            "{size:?}"
        );
    }
}

#[test]
fn a_style_amendment_reshapes_the_ring() {
    crate::test_support::fresh_layout_runtime();
    let square: telar::SurfaceAmendment =
        Rc::new(|s: RectStyle| s.with_radius(BorderRadius::all(0.0)));
    let [(_, ring), (_, dot)] = ring_and_dot(chosen(ControlSize::Regular, Some(square)))[..] else {
        panic!("a chosen radio paints its ring and its dot");
    };
    assert_eq!(ring.radius, BorderRadius::all(0.0));
    assert_ne!(
        dot.radius,
        BorderRadius::all(0.0),
        "the dot is not the principal surface"
    );
}
