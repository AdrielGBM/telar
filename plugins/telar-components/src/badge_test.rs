use telar::{AvailableSpace, ComponentList, DrawCommand, compute_layout, new_container};

use super::*;
use crate::test_support::find_text;

fn laid_out(item: Box<dyn LayoutItem>) -> ComponentList {
    let root = new_container(
        LayoutStyle::new().flex_row().width(200.0).height(60.0),
        &[item.layout_node()],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(60.0),
    )
    .unwrap();
    ComponentList::new(item)
}

#[test]
fn renders_label() {
    crate::test_support::fresh_layout_runtime();
    let badge = badge(BadgeProps::props().label("New").build()).unwrap();
    let tree = laid_out(badge);
    assert!(
        find_text(&tree.commands(), "New"),
        "a badge draws its label"
    );
}

#[test]
fn empty_label_builds_without_panic() {
    crate::test_support::fresh_layout_runtime();
    let badge = badge(BadgeProps::props().build()).unwrap();
    let tree = laid_out(badge);
    let _ = tree.commands();
}

fn mounted(props: BadgeProps) -> (ComponentList, telar::Rect) {
    crate::test_support::fresh_layout_runtime();
    let badge = badge(props).unwrap();
    let rect = telar::track_layout(badge.layout_node()).unwrap();
    compute_layout(
        badge.layout_node(),
        AvailableSpace::MaxContent,
        AvailableSpace::MaxContent,
    )
    .unwrap();
    (ComponentList::new(badge), rect.get())
}

#[test]
fn a_badge_is_a_pill_until_the_badge_tokens_name_a_radius() {
    let laid_out = std::rc::Rc::new(std::cell::Cell::new(Rect::default()));
    let at = laid_out.clone();
    crate::test_support::assert_corner_follows_the_role_until_the_tokens_name_one(
        move || {
            let (tree, rect) = mounted(BadgeProps::props().label("New").build());
            at.set(rect);
            tree
        },
        |tree| crate::test_support::first_painted(tree).radius,
        move |_, _| shared::pill(laid_out.get()),
        |radius| BadgeTokens {
            radius: Some(radius),
            ..BadgeTokens::default()
        },
    );
}

#[test]
fn a_style_amendment_reshapes_the_pill() {
    let flat: SurfaceAmendment =
        std::rc::Rc::new(|s: RectStyle| s.with_radius(BorderRadius::all(0.0)));
    let (tree, _) = mounted(BadgeProps::props().label("New").style(flat).build());
    let painted = crate::test_support::first_painted(&tree);
    assert_eq!(painted.radius, BorderRadius::all(0.0));
    assert!(painted.fill.is_some(), "the rest of the paint is kept");
}

/// A badge's padding is a length at the regular size, so a larger control size pads it in proportion.
#[test]
fn the_padding_scales_with_the_control_size() {
    crate::test_support::fresh_layout_runtime();
    let badge = telar::provide_control_size(telar::ControlSize::Large, || {
        badge(BadgeProps::props().label("New").build())
    })
    .unwrap();
    let rect = telar::track_layout(badge.layout_node()).unwrap();
    compute_layout(
        badge.layout_node(),
        AvailableSpace::MaxContent,
        AvailableSpace::MaxContent,
    )
    .unwrap();
    let tree = ComponentList::new(badge);
    let label = tree
        .commands()
        .iter()
        .find_map(|c| match c {
            DrawCommand::Text { text, rect, .. } if text.as_ref() == "New" => Some(*rect),
            _ => None,
        })
        .expect("the badge draws its label");
    let padding = rect.get().width - label.width;
    let expected = 2.0 * shared::gap_md() * 1.25;
    assert!(
        (padding - expected).abs() <= 0.5,
        "padded by {padding}, not {expected}, within the half pixel layout rounds the box by"
    );
}
