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
        Children::default(),
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
        Children::default(),
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
        Children::default(),
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
        Children::default(),
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
    assert_eq!(glyph_for("lucide:search"), "⌕");
    assert_eq!(glyph_for("mdi:search"), "⌕");
    assert_eq!(glyph_for("search"), "⌕");
}

#[test]
fn an_icon_with_no_stand_in_still_gets_a_body() {
    assert_eq!(glyph_for("lucide:no-such-icon"), "•");
}

#[cfg(feature = "icons")]
#[test]
fn an_icon_baked_from_rust_draws_its_artwork() {
    crate::test_support::fresh_layout_runtime();
    focus::clear();
    let button = icon_button(
        IconButtonProps::props()
            .icon(telar_icons::icon!("lucide:search"))
            .label("Search")
            .build(),
        Children::default(),
    )
    .unwrap();
    let tree = mount(button);
    let commands = tree.commands();
    assert!(
        commands
            .iter()
            .any(|command| matches!(command, telar::DrawCommand::Path { .. })),
        "{commands:?}"
    );
}

fn mounted(props: IconButtonProps) -> ComponentList {
    crate::test_support::fresh_layout_runtime();
    mount(icon_button(props, Children::default()).unwrap())
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
