use std::cell::Cell;

use super::*;
use telar::{
    AvailableSpace, ComponentList, Container, Event, Key, NamedKey, compute_layout, focus, signal,
};

fn mount(item: Box<dyn LayoutItem>) -> ComponentList {
    let root = Container::new(
        LayoutStyle::new().flex_column().width(300.0).height(100.0),
        vec![item],
    )
    .unwrap();
    compute_layout(
        root.layout_node(),
        AvailableSpace::Definite(300.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();
    let tree = ComponentList::new(box_item(root));
    let _ = tree.commands();
    tree
}

fn enter() -> Event {
    Event::KeyPressed {
        key: Key::Named(NamedKey::Enter),
        modifiers: telar::ModifiersState::default(),
        unmodified: None,
    }
}

fn exposed_button() -> focus::Exposed {
    focus::exposed()
        .into_iter()
        .find(|exposed| exposed.role == Role::Button)
        .expect("the icon button is exposed as a button")
}

#[test]
fn a_standalone_icon_button_is_a_tab_stop_that_enter_presses() {
    let fired = Rc::new(Cell::new(0));
    crate::test_support::fresh_layout_runtime();
    focus::clear();
    let sink = fired.clone();
    let button = icon_button(
        IconButtonProps::props()
            .icon("lucide:search")
            .label("Search")
            .on_press(Rc::new(move || sink.set(sink.get() + 1)))
            .build(),
    )
    .unwrap();
    let mut tree = mount(button);

    focus::focus_next();
    assert!(focus::current().is_some());
    tree.on_event(&enter());
    assert_eq!(fired.get(), 1);
}

#[test]
fn a_plain_button_says_nothing_about_being_pressed() {
    crate::test_support::fresh_layout_runtime();
    focus::clear();
    let button = icon_button(
        IconButtonProps::props()
            .icon("lucide:search")
            .label("Search")
            .build(),
    )
    .unwrap();
    let _tree = mount(button);

    assert_eq!(exposed_button().toggled, None);
}

#[test]
fn the_pressed_state_follows_its_signal_into_the_semantics() {
    crate::test_support::fresh_layout_runtime();
    focus::clear();
    let on = signal(false);
    let button = icon_button(
        IconButtonProps::props()
            .icon("lucide:grid")
            .label("Grid")
            .pressed(on)
            .build(),
    )
    .unwrap();
    let _tree = mount(button);

    assert_eq!(exposed_button().toggled, Some(false));
    on.set(true);
    assert_eq!(exposed_button().toggled, Some(true));
}

#[test]
fn a_disabled_button_is_announced_as_unavailable_and_does_not_fire() {
    let fired = Rc::new(Cell::new(false));
    crate::test_support::fresh_layout_runtime();
    focus::clear();
    let sink = fired.clone();
    let button = icon_button(
        IconButtonProps::props()
            .icon("lucide:search")
            .label("Search")
            .disabled(true)
            .on_press(Rc::new(move || sink.set(true)))
            .build(),
    )
    .unwrap();
    let mut tree = mount(button);

    assert!(!exposed_button().enabled);
    focus::focus_next();
    tree.on_event(&enter());
    assert!(!fired.get());
}

#[test]
fn the_fallback_glyph_depends_on_the_name_and_not_the_set() {
    assert_eq!(glyph_for("lucide:search"), "⚲");
    assert_eq!(glyph_for("mdi:search"), "⚲");
    assert_eq!(glyph_for("search"), "⚲");
}

#[test]
fn the_icons_a_control_is_commonly_handed_have_a_stand_in() {
    assert_eq!(glyph_for("lucide:x"), "✕");
    assert_eq!(glyph_for("lucide:external-link"), "↗");
    assert_eq!(glyph_for("lucide:chevron-left"), "‹");
    assert_eq!(glyph_for("lucide:chevron-right"), "›");
    assert_eq!(glyph_for("lucide:chevron-up"), "˄");
    assert_eq!(glyph_for("lucide:chevron-down"), "˅");
}

#[test]
fn an_icon_with_no_stand_in_still_gets_a_body() {
    assert_eq!(glyph_for("lucide:no-such-icon"), "•");
}

fn drawn_with(icon: impl Into<IconName>) -> Vec<telar::DrawCommand> {
    crate::test_support::fresh_layout_runtime();
    focus::clear();
    let button = icon_button(IconButtonProps::props().icon(icon).label("Icon").build()).unwrap();
    mount(button).commands().to_vec()
}

#[cfg(feature = "icons")]
fn draws(commands: &[telar::DrawCommand], _id: &str) -> bool {
    commands
        .iter()
        .any(|command| matches!(command, telar::DrawCommand::Path { .. }))
}

#[cfg(not(feature = "icons"))]
fn draws(commands: &[telar::DrawCommand], id: &str) -> bool {
    commands.iter().any(|command| {
        matches!(command, telar::DrawCommand::Text { text, .. } if text.as_ref() == glyph_for(id))
    })
}

#[test]
fn an_icon_named_from_rust_draws_its_artwork_or_its_stand_in() {
    let commands = drawn_with(telar::icon!("lucide:search"));
    assert!(draws(&commands, "lucide:search"), "{commands:?}");
}

/// `icon_button icon:"lucide:x"` in a package that bakes icons hands the prop the artifact's triple, which the one prop type takes whether or not `telar-icons` is in the build.
#[test]
fn the_triple_a_baked_rsx_literal_becomes_draws_in_every_build() {
    let commands = drawn_with(telar::__baked_id!(icon, "lucide:x", telar));
    assert!(draws(&commands, "lucide:x"), "{commands:?}");
}

#[cfg(not(feature = "icons"))]
#[test]
fn an_unknown_name_draws_a_dot_without_icons() {
    let commands = drawn_with("lucide:no-such-icon");
    assert!(
        commands.iter().any(
            |command| matches!(command, telar::DrawCommand::Text { text, .. } if text.as_ref() == "•")
        ),
        "{commands:?}"
    );
}

fn mounted(props: IconButtonProps) -> ComponentList {
    crate::test_support::fresh_layout_runtime();
    mount(icon_button(props).unwrap())
}

fn square(tree: &ComponentList) -> RectStyle {
    crate::test_support::painted_at(tree, crate::test_support::drawn(Role::Button)[0])
}

#[test]
fn the_corner_is_the_theme_s_control_role_until_the_icon_button_tokens_name_one() {
    crate::test_support::assert_corner_follows_the_role_until_the_tokens_name_one(
        || {
            mounted(
                IconButtonProps::props()
                    .icon("lucide:search")
                    .label("Search")
                    .build(),
            )
        },
        |tree| square(tree).radius,
        |_, role| BorderRadius::all(role),
        |radius| IconButtonTokens {
            radius: Some(radius),
        },
    );
}

#[test]
fn a_toggle_held_on_wears_the_theme_s_held_wash() {
    let tree = mounted(
        IconButtonProps::props()
            .icon("lucide:star")
            .label("Favourite")
            .pressed(true)
            .build(),
    );
    assert_eq!(
        square(&tree).fill.map(|paint| paint.solid_color()),
        Some(telar::use_theme_tokens().highlight_med())
    );
}

/// `@class` and the `style` prop amend the finished paint, so they reach the hovered square as well as the resting one.
#[test]
fn a_style_amendment_reshapes_every_state() {
    use telar::testing::{centre, moved, route};

    let flat: SurfaceAmendment = Rc::new(|s: RectStyle| s.with_radius(BorderRadius::all(0.0)));
    let mut tree = mounted(
        IconButtonProps::props()
            .icon("lucide:search")
            .label("Search")
            .style(flat)
            .build(),
    );
    assert_eq!(square(&tree).radius, BorderRadius::all(0.0));
    let (x, y) = centre(crate::test_support::drawn(Role::Button)[0]);
    route(&mut tree, &moved(x, y));
    let hovered = square(&tree);
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

/// The key binding a button's tooltip shows belongs to the button it activates, not to the bubble, and is told to a reader in the form it takes.
#[test]
fn the_shortcut_is_the_button_s_own_and_not_the_tooltip_s() {
    focus::clear();
    let tree = telar::testing::scoped(|| {
        mounted(
            IconButtonProps::props()
                .icon("lucide:search")
                .label("Search")
                .shortcut("Mod+K")
                .build(),
        )
    });
    assert_eq!(
        exposed_button().keyboard_shortcut.as_deref(),
        Some("Control+K")
    );
    drop(tree);

    focus::clear();
    let _tree = mounted(
        IconButtonProps::props()
            .icon("lucide:search")
            .label("Search")
            .build(),
    );
    assert_eq!(exposed_button().keyboard_shortcut, None);
}

/// The square under the tooltip is a recipe, so built again it is the same working button rather than an empty trigger.
#[test]
fn the_square_under_the_tooltip_builds_again_as_a_working_button() {
    let fired = Rc::new(Cell::new(0));
    crate::test_support::fresh_layout_runtime();
    focus::clear();
    let sink = fired.clone();
    let square = Square {
        icon: "lucide:search".into(),
        label: "Search".into(),
        size: crate::ControlSize::Regular,
        shortcut: Reactive::default(),
        pressed: None,
        disabled: Reactive::default(),
        activate: Rc::new(move || sink.set(sink.get() + 1)),
        style: None,
        toolbar: None,
        index: None,
    };
    let first = square.build().unwrap();
    let again = square.build().unwrap();
    let tree = mount(box_item(
        Container::new(LayoutStyle::new().flex_row(), vec![first, again]).unwrap(),
    ));
    let named: Vec<String> = telar::inspect::snapshot(&tree.commands())
        .into_iter()
        .filter(|node| node.role == Role::Button)
        .map(|node| node.name)
        .collect();
    assert_eq!(named, ["Search", "Search"]);

    let mut tree = tree;
    focus::focus_next();
    focus::focus_next();
    tree.on_event(&enter());
    assert_eq!(fired.get(), 1, "the second build presses like the first");
}

/// Lays `item` out alone in the middle of a 200×80 window, with room around it for a pointer to land past its edges.
fn centred(item: Box<dyn LayoutItem>) -> ComponentList {
    let root = Container::new(
        LayoutStyle::row()
            .width(200.0)
            .height(80.0)
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER),
        vec![item],
    )
    .unwrap();
    compute_layout(
        root.layout_node(),
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(80.0),
    )
    .unwrap();
    let tree = ComponentList::new(box_item(root));
    let _ = tree.commands();
    tree
}

fn sized(size: ControlSize, run: Rc<dyn Fn()>) -> ComponentList {
    crate::test_support::fresh_layout_runtime();
    focus::clear();
    centred(
        icon_button(
            IconButtonProps::props()
                .icon("lucide:x")
                .label("Remove")
                .size(size)
                .on_press(run)
                .build(),
        )
        .unwrap(),
    )
}

/// The square is 24, 32 and 40 px across from small to large, the smallest pointer target at its least, and the icon in it follows the size too.
#[test]
fn the_square_and_its_icon_follow_the_size() {
    for (size, side) in [
        (ControlSize::Small, 24.0),
        (ControlSize::Regular, 32.0),
        (ControlSize::Large, 40.0),
    ] {
        let tree = sized(size, Rc::new(|| {}));
        let face = crate::test_support::drawn(Role::Button)[0];
        assert_eq!((face.width, face.height), (side, side), "{size:?}");
        assert!(
            draws_its_icon_at(&tree, size),
            "{size:?} draws its icon at its size"
        );
    }
}

#[cfg(feature = "icons")]
fn draws_its_icon_at(tree: &ComponentList, size: ControlSize) -> bool {
    let icon = crate::metrics::icon_size(size);
    tree.commands().iter().any(|command| match command {
        telar::DrawCommand::Rect { rect, .. } => {
            (rect.width - icon).abs() < 0.5 && (rect.height - icon).abs() < 0.5
        }
        _ => false,
    })
}

#[cfg(not(feature = "icons"))]
fn draws_its_icon_at(tree: &ComponentList, size: ControlSize) -> bool {
    let font_size = telar::Inherited::initial().text.font_size * size.text_scale();
    tree.commands().iter().any(|command| match command {
        telar::DrawCommand::Text { text, style, .. } => {
            text.as_ref() == glyph_for("lucide:x") && (style.font_size - font_size).abs() < 0.5
        }
        _ => false,
    })
}
