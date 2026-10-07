use telar::{
    AvailableSpace, ComponentList, Declared, DrawCommand, NodeId, Paint, Rect, Role,
    compute_layout, declare, new_container, reset_layout_runtime, track_layout,
};
use telar_dynamic::{AssetDecoder, SvgDecoder};

use super::*;

const MONOTONE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24"><path fill="currentColor" d="M3 11L12 3l9 8v10H3z"/></svg>"#;
const PALETTE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24"><rect width="12" height="24" fill="#ff0000"/><rect x="12" width="12" height="24" fill="#0000ff"/></svg>"##;
const BRAND: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24"><circle cx="12" cy="12" r="10" fill="#1877f2"/></svg>"##;

fn svg(source: &str) -> Arc<SvgData> {
    SvgDecoder.decode(source.as_bytes()).unwrap()
}

fn baked(source: &str) -> IconName {
    IconName::from(("demo:glyph", svg(source), true))
}

fn baked_in_palette(source: &str) -> IconName {
    IconName::from(("demo:glyph", svg(source), false))
}

/// `item` laid out in a row that declares `ink` as its text colour, with the icon's own box tracked.
struct Laid {
    tree: ComponentList,
    rect: Rect,
}

fn lay_out(item: Box<dyn LayoutItem>, ink: Color) -> Laid {
    let rect = track_layout(item.layout_node()).unwrap();
    let root: NodeId = new_container(
        LayoutStyle::new().flex_row().width(200.0).height(100.0),
        &[item.layout_node()],
    )
    .unwrap();
    declare(
        root,
        Declared {
            color: Some(Paint::Solid(ink)),
            ..Declared::default()
        },
    );
    compute_layout(
        root,
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();
    Laid {
        rect: rect.get(),
        tree: ComponentList::new(item),
    }
}

fn path_fills(commands: &[DrawCommand]) -> Vec<Paint> {
    commands
        .iter()
        .filter_map(|command| match command {
            DrawCommand::Path { style, .. } => style.fill,
            _ => None,
        })
        .collect()
}

#[test]
fn a_baked_icon_draws_its_path_in_the_text_colour_around_it() {
    reset_layout_runtime();
    let item = icon(
        IconProps::props().name(baked(MONOTONE)).size(24.0).build(),
        Children::default(),
    )
    .unwrap();
    let laid = lay_out(item, Color::RED);
    assert_eq!(
        path_fills(&laid.tree.commands()),
        vec![Paint::Solid(Color::RED)]
    );
    assert_eq!((laid.rect.width, laid.rect.height), (24.0, 24.0));
}

#[test]
fn a_colour_given_wins_over_the_text_colour() {
    reset_layout_runtime();
    let item = icon(
        IconProps::props()
            .name(baked(MONOTONE))
            .color(Color::BLUE)
            .build(),
        Children::default(),
    )
    .unwrap();
    let laid = lay_out(item, Color::RED);
    assert_eq!(
        path_fills(&laid.tree.commands()),
        vec![Paint::Solid(Color::BLUE)]
    );
}

#[test]
fn a_multicolour_icon_keeps_its_colours() {
    reset_layout_runtime();
    let item = icon(
        IconProps::props().name(baked_in_palette(PALETTE)).build(),
        Children::default(),
    )
    .unwrap();
    let fills = path_fills(&lay_out(item, Color::GREEN).tree.commands());
    assert_eq!(fills.len(), 2, "{fills:?}");
    assert!(!fills.contains(&Paint::Solid(Color::GREEN)), "{fills:?}");
}

#[test]
fn an_unset_size_is_the_themes_icon_size() {
    reset_layout_runtime();
    let item = icon(
        IconProps::props().name(baked(MONOTONE)).build(),
        Children::default(),
    )
    .unwrap();
    let expected = use_theme_tokens().icon_size();
    let rect = lay_out(item, Color::RED).rect;
    assert_eq!((rect.width, rect.height), (expected, expected));
}

#[test]
fn an_icon_is_decoration_unless_it_is_named() {
    reset_layout_runtime();
    let decorative = icon(
        IconProps::props().name(baked(MONOTONE)).build(),
        Children::default(),
    )
    .unwrap();
    let laid = lay_out(decorative, Color::RED);
    assert!(ui_core::accessibility::snapshot(&laid.tree.commands()).is_empty());

    reset_layout_runtime();
    let named = icon(
        IconProps::props()
            .name(baked(MONOTONE))
            .label("Home")
            .build(),
        Children::default(),
    )
    .unwrap();
    let laid = lay_out(named, Color::RED);
    let nodes = ui_core::accessibility::snapshot(&laid.tree.commands());
    assert_eq!(nodes.len(), 1, "{nodes:?}");
    assert_eq!(nodes[0].name, "Home");
    assert_eq!(nodes[0].role, Role::Drawing);
}

#[test]
fn an_id_nothing_resolves_keeps_its_box_and_draws_nothing() {
    reset_layout_runtime();
    let item = icon(
        IconProps::props().name("demo:unbaked").size(20.0).build(),
        Children::default(),
    )
    .unwrap();
    let laid = lay_out(item, Color::RED);
    assert!(path_fills(&laid.tree.commands()).is_empty());
    assert_eq!((laid.rect.width, laid.rect.height), (20.0, 20.0));
}

fn brand_blue() -> Color {
    Color::from_hex("#1877f2").unwrap()
}

#[test]
fn a_one_colour_logo_of_a_palette_set_keeps_its_colour() {
    reset_layout_runtime();
    let item = icon(
        IconProps::props()
            .name(baked_in_palette(BRAND))
            .size(24.0)
            .build(),
        Children::default(),
    )
    .unwrap();
    let fills = path_fills(&lay_out(item, Color::RED).tree.commands());
    assert_eq!(fills.len(), 1, "{fills:?}");
    let Paint::Solid(fill) = fills[0] else {
        panic!("{fills:?}");
    };
    assert_ne!(fill, Color::RED, "{fills:?}");
    assert!((fill.b - brand_blue().b).abs() < 0.01, "{fills:?}");
}

#[test]
fn a_monochrome_icon_is_tinted_whatever_colour_it_is_drawn_in() {
    reset_layout_runtime();
    let item = icon(
        IconProps::props().name(baked(BRAND)).size(24.0).build(),
        Children::default(),
    )
    .unwrap();
    assert_eq!(
        path_fills(&lay_out(item, Color::RED).tree.commands()),
        vec![Paint::Solid(Color::RED)]
    );
}

#[test]
fn a_colour_given_tints_even_a_palette_icon() {
    reset_layout_runtime();
    let item = icon(
        IconProps::props()
            .name(baked_in_palette(PALETTE))
            .color(Color::BLUE)
            .build(),
        Children::default(),
    )
    .unwrap();
    let fills = path_fills(&lay_out(item, Color::RED).tree.commands());
    assert_eq!(fills.len(), 2, "{fills:?}");
    assert!(
        fills.iter().all(|fill| *fill == Paint::Solid(Color::BLUE)),
        "{fills:?}"
    );
}
