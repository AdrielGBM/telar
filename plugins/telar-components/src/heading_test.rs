use std::sync::Arc;

use telar::{
    AvailableSpace, Children, ComponentList, ControlSize, DrawCommand, LayoutItem, Role, TextStyle,
    compute_layout,
};

use crate::{HeadingProps, heading};

fn drawn(item: Box<dyn LayoutItem>) -> ComponentList {
    compute_layout(
        item.layout_node(),
        AvailableSpace::MaxContent,
        AvailableSpace::MaxContent,
    )
    .unwrap();
    ComponentList::new(item)
}

fn title_style(item: Box<dyn LayoutItem>) -> Arc<TextStyle> {
    drawn(item)
        .commands()
        .iter()
        .find_map(|command| match command {
            DrawCommand::Text { style, .. } => Some(style.clone()),
            _ => None,
        })
        .expect("the heading draws its text")
}

fn exposed_roles(tree: &ComponentList) -> Vec<Role> {
    ui_core::accessibility::snapshot(&tree.commands())
        .into_iter()
        .map(|node| node.role)
        .collect()
}

fn titled(level: u8) -> Box<dyn LayoutItem> {
    heading(
        HeadingProps::props().text("Account").level(level).build(),
        Children::default(),
    )
    .unwrap()
}

fn body() -> TextStyle {
    telar::Inherited::initial().text
}

#[test]
fn a_heading_takes_the_ink_around_it_in_the_strong_weight() {
    crate::test_support::fresh_layout_runtime();
    let style = title_style(titled(2));
    assert_eq!(
        style.font_weight,
        telar::use_theme_tokens().font_weight_strong()
    );
    assert_eq!(style.color, body().color);
}

#[test]
fn a_heading_is_sized_by_its_level_against_the_text_around_it() {
    let scales: Vec<f32> = (1..=6)
        .map(|level| {
            crate::test_support::fresh_layout_runtime();
            title_style(titled(level)).font_size / body().font_size
        })
        .collect();
    assert_eq!(scales, [2.0, 1.5, 1.25, 1.0, 0.875, 0.75]);
}

/// A level outside the outline is the nearest one in it.
#[test]
fn a_heading_is_the_heading_of_its_level_to_assistive_technology() {
    for (level, expected) in [(1, 1), (4, 4), (0, 1), (9, 6)] {
        crate::test_support::fresh_layout_runtime();
        let roles = exposed_roles(&drawn(titled(level)));
        assert!(
            roles.contains(&Role::Heading(expected)),
            "level {level} is read as h{expected}: {roles:?}"
        );
    }
}

#[test]
fn a_heading_is_text_and_keeps_its_size_whatever_the_control_size() {
    crate::test_support::fresh_layout_runtime();
    let regular = title_style(titled(2)).font_size;
    crate::test_support::fresh_layout_runtime();
    let large = telar::provide_control_size(ControlSize::Large, || Ok(titled(2))).unwrap();
    assert_eq!(title_style(Box::new(large)).font_size, regular);
}

/// What the call authors replaces what the heading chose, and what it does not name stays the heading's: a lighter `h1` keeps its size, and a size in `em` is of the text around it.
#[test]
fn what_the_call_authors_wins_over_the_heading_s_own_style() {
    use telar::{AuthoredText, Declared, TextLength};
    crate::test_support::fresh_layout_runtime();
    let lighter = titled(1).authored_text(|| Declared::default().with_font_weight(400));
    let style = title_style(Box::new(lighter));
    assert_eq!(
        (style.font_weight, style.font_size),
        (400, body().font_size * 2.0)
    );
    crate::test_support::fresh_layout_runtime();
    let smaller =
        titled(1).authored_text(|| Declared::default().with_font_size(TextLength::Em(1.25)));
    assert_eq!(
        title_style(Box::new(smaller)).font_size,
        body().font_size * 1.25
    );
}

/// Every level returns the text itself, so what a call authors lands on it.
#[test]
fn what_the_call_authors_lands_on_every_level() {
    use telar::{AuthoredText, Declared};
    for level in 1..=6 {
        crate::test_support::fresh_layout_runtime();
        let lighter = titled(level).authored_text(|| Declared::default().with_font_weight(300));
        assert_eq!(title_style(Box::new(lighter)).font_weight, 300, "h{level}");
    }
}
