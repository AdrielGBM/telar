use super::*;
use crate::test_support::{find_text, tall_body, text_body};
use telar::{
    AvailableSpace, ComponentList, DrawCommand, compute_layout, new_container, relayout_if_dirty,
    signal,
};

#[test]
fn open_shows_panel_and_close_hides_it() {
    crate::test_support::fresh_layout_runtime();
    let open = signal(false);
    let drawer = drawer(
        DrawerProps::props()
            .control(open)
            .side(Side::End)
            .children(text_body("Drawer body"))
            .build(),
    )
    .unwrap();

    let root = new_container(
        LayoutStyle::new().flex_column().width(400.0).height(400.0),
        &[drawer.layout_node()],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    let tree = ComponentList::new(drawer);

    assert!(
        !find_text(&tree.commands(), "Drawer body"),
        "closing hides the panel again"
    );

    open.set(true);
    relayout_if_dirty();
    assert!(
        find_text(&tree.commands(), "Drawer body"),
        "body shows when open"
    );

    open.set(false);
    relayout_if_dirty();
    assert!(
        !find_text(&tree.commands(), "Drawer body"),
        "body hidden when closed"
    );
}

const FILTERS: telar::OverlayName = telar::OverlayName::new("filters-test");

#[test]
fn a_named_drawer_opens_and_closes_by_its_name() {
    crate::test_support::fresh_layout_runtime();
    telar::close_overlay(FILTERS);
    let drawer = drawer(
        DrawerProps::props()
            .control(FILTERS)
            .children(text_body("Drawer body"))
            .build(),
    )
    .unwrap();
    let root = new_container(
        LayoutStyle::new().flex_column().width(400.0).height(400.0),
        &[drawer.layout_node()],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    let tree = ComponentList::new(drawer);
    assert!(
        !find_text(&tree.commands(), "Drawer body"),
        "a closed name draws nothing"
    );

    telar::open_overlay(FILTERS);
    relayout_if_dirty();
    assert!(
        find_text(&tree.commands(), "Drawer body"),
        "opening the name shows the panel"
    );

    telar::close_overlay(FILTERS);
    relayout_if_dirty();
    assert!(
        !find_text(&tree.commands(), "Drawer body"),
        "closing the name hides it"
    );
}

#[test]
fn a_named_drawer_is_a_dialog_to_a_reader() {
    crate::test_support::fresh_layout_runtime();
    let open = signal(true);
    let drawer = drawer(
        DrawerProps::props()
            .control(open)
            .label("Filters")
            .children(text_body("Drawer body"))
            .build(),
    )
    .unwrap();
    let root = new_container(
        LayoutStyle::new().flex_column().width(400.0).height(400.0),
        &[drawer.layout_node()],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    let tree = ComponentList::new(drawer);
    relayout_if_dirty();

    let nodes = telar::inspect::snapshot(&tree.commands());
    assert!(
        nodes
            .iter()
            .any(|node| node.role == Role::Dialog && node.name == "Filters")
    );
}

fn open_drawer(props: DrawerProps) -> telar::testing::Scoped<ComponentList> {
    crate::test_support::fresh_layout_runtime();
    let tree = telar::testing::scoped(|| {
        let drawer = drawer(DrawerProps {
            children: text_body("Drawer body"),
            ..props
        })
        .unwrap();
        let root = new_container(
            LayoutStyle::new().flex_column().width(400.0).height(400.0),
            &[drawer.layout_node()],
        )
        .unwrap();
        compute_layout(
            root,
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(400.0),
        )
        .unwrap();
        ComponentList::new(drawer)
    });
    relayout_if_dirty();
    tree
}

fn panel(tree: &ComponentList) -> (telar::Rect, RectStyle) {
    let nodes = telar::inspect::snapshot(&tree.commands());
    let panel = nodes
        .iter()
        .find(|node| node.role == Role::Dialog)
        .expect("the drawer is open");
    (
        panel.rect,
        crate::test_support::painted_at(tree, panel.rect),
    )
}

fn rounded(left: f32, right: f32) -> BorderRadius {
    BorderRadius {
        top_left: left,
        top_right: right,
        bottom_right: right,
        bottom_left: left,
    }
}

#[test]
fn the_panel_rounds_the_side_facing_the_page_by_the_dialog_role() {
    crate::test_support::assert_panel_corner_follows_its_role(
        crate::test_support::PanelRole::Dialog,
        || open_drawer(DrawerProps::props().control(signal(true)).build()),
        |tree| panel(tree).1.radius,
        |radius| rounded(0.0, radius),
        |radius| DrawerTokens {
            radius: Some(radius),
            ..DrawerTokens::default()
        },
    );
}

#[test]
fn a_panel_pinned_to_the_right_rounds_its_left_corners() {
    telar::set_theme(crate::test_support::PanelTheme::<DrawerTokens> {
        role: crate::test_support::PanelRole::Dialog,
        radius: 9.0,
        tokens: None,
    });
    let tree = open_drawer(
        DrawerProps::props()
            .control(signal(true))
            .side(Side::End)
            .build(),
    );
    let (rect, paint) = panel(&tree);
    assert_eq!(rect.x + rect.width, 400.0, "pinned to the right edge");
    assert_eq!(paint.radius, rounded(9.0, 0.0));
}

/// Which edge of a 400 × 400 surface reading `direction` a drawer on `side` is pinned to, by the edges its panel touches.
fn pinned_edges(side: Side, direction: telar::Direction) -> (bool, bool, bool, bool) {
    telar::set_direction(direction);
    let tree = open_drawer(
        DrawerProps::props()
            .control(signal(true))
            .side(side)
            .build(),
    );
    let rect = panel(&tree).0;
    telar::set_direction(telar::Direction::Ltr);
    (
        rect.y == 0.0,
        rect.x + rect.width == 400.0,
        rect.y + rect.height == 400.0,
        rect.x == 0.0,
    )
}

#[test]
fn a_drawer_at_the_end_opens_on_the_right_of_a_left_to_right_surface_and_on_the_left_of_a_right_to_left_one()
 {
    let (top, right, bottom, left) = pinned_edges(Side::End, telar::Direction::Ltr);
    assert!(top && right && bottom && !left);
    let (top, right, bottom, left) = pinned_edges(Side::End, telar::Direction::Rtl);
    assert!(top && !right && bottom && left);
    let (_, right, _, left) = pinned_edges(Side::Start, telar::Direction::Rtl);
    assert!(right && !left, "and the start the other way round");
}

#[test]
fn a_drawer_above_or_below_spans_the_surface_s_width_at_its_top_or_bottom() {
    let (top, right, bottom, left) = pinned_edges(Side::Above, telar::Direction::Ltr);
    assert!(top && right && !bottom && left);
    let (top, right, bottom, left) = pinned_edges(Side::Below, telar::Direction::Ltr);
    assert!(!top && right && bottom && left);
}

#[test]
fn a_transparent_panel_is_drawn_transparent_and_an_absent_colour_is_the_surface() {
    let tree = open_drawer(
        DrawerProps::props()
            .control(signal(true))
            .color(Color::TRANSPARENT)
            .build(),
    );
    assert_eq!(
        panel(&tree).1.fill.map(|paint| paint.solid_color()),
        Some(Color::TRANSPARENT)
    );
    drop(tree);
    let tree = open_drawer(DrawerProps::props().control(signal(true)).build());
    assert_eq!(
        panel(&tree).1.fill.map(|paint| paint.solid_color()),
        Some(shared::surface())
    );
}

#[test]
fn an_unset_extent_is_the_tokens_and_a_named_one_wins() {
    let tokens = DrawerTokens {
        extent: Some(250.0),
        ..DrawerTokens::default()
    };
    telar::set_theme(crate::test_support::PanelTheme {
        role: crate::test_support::PanelRole::Dialog,
        radius: 9.0,
        tokens: Some(tokens),
    });
    let tree = open_drawer(DrawerProps::props().control(signal(true)).build());
    assert_eq!(panel(&tree).0.width, 250.0);
    drop(tree);

    let tree = open_drawer(
        DrawerProps::props()
            .control(signal(true))
            .extent(180.0)
            .build(),
    );
    assert_eq!(panel(&tree).0.width, 180.0, "the drawer's own extent wins");
}

/// An unset panel is the measure of the drawer's own size wide, and a width its tokens name at the regular size scales with the text to it.
#[test]
fn the_panel_s_width_follows_the_drawer_s_size() {
    telar::set_theme(crate::test_support::PanelTheme::<DrawerTokens> {
        role: crate::test_support::PanelRole::Dialog,
        radius: 9.0,
        tokens: None,
    });
    for size in [ControlSize::Small, ControlSize::Regular, ControlSize::Large] {
        let tree = open_drawer(
            DrawerProps::props()
                .control(signal(true))
                .size(size)
                .build(),
        );
        assert_eq!(
            panel(&tree).0.width,
            crate::metrics::measure(size, 20.0),
            "{size:?}"
        );
    }
    let tokens = DrawerTokens {
        extent: Some(250.0),
        pad: Some(10.0),
        ..DrawerTokens::default()
    };
    assert_eq!(tokens.extent(ControlSize::Large), (250.0 * 1.125f32).round());
    assert_eq!(tokens.pad(ControlSize::Large), 12.5);
}

#[test]
fn a_style_amendment_and_the_overlay_role_reach_the_open_drawer() {
    use telar::ThemeTokens;

    #[derive(Clone)]
    struct Dimmed;
    impl ThemeTokens for Dimmed {
        fn overlay(&self) -> Color {
            Color::rgba(0.2, 0.1, 0.0, 0.6)
        }
    }
    telar::set_theme(Dimmed);
    let ink = Color::rgb(0.1, 0.5, 0.9);
    let tree = open_drawer(
        DrawerProps::props()
            .control(signal(true))
            .style(Rc::new(move |paint: RectStyle| paint.with_fill(ink)) as telar::SurfaceAmendment)
            .build(),
    );
    assert_eq!(panel(&tree).1.fill, Some(ink.into()));
    assert!(tree.commands().iter().any(|command| matches!(
        command,
        DrawCommand::Rect { style, .. } if style.fill == Some(Dimmed.overlay().into())
    )));
}

fn open_drawer_in(
    window: (f32, f32),
    body: Children,
    props: DrawerProps,
) -> telar::testing::Scoped<ComponentList> {
    crate::test_support::fresh_layout_runtime();
    let tree = telar::testing::scoped(|| {
        let drawer = drawer(DrawerProps {
            children: body,
            ..props
        })
        .unwrap();
        let root = new_container(
            LayoutStyle::new()
                .flex_column()
                .width(window.0)
                .height(window.1),
            &[drawer.layout_node()],
        )
        .unwrap();
        compute_layout(
            root,
            AvailableSpace::Definite(window.0),
            AvailableSpace::Definite(window.1),
        )
        .unwrap();
        ComponentList::new(drawer)
    });
    relayout_if_dirty();
    tree
}

#[test]
fn a_panel_wider_than_the_window_is_cut_to_the_window() {
    for width in [150.0, 280.0] {
        let tree = open_drawer_in(
            (width, 400.0),
            text_body("Drawer body"),
            DrawerProps::props()
                .control(signal(true))
                .extent(500.0)
                .build(),
        );
        let rect = panel(&tree).0;
        assert!(rect.x >= 0.0 && rect.x + rect.width <= width, "{rect:?}");
    }
}

#[test]
fn a_tall_body_scrolls_inside_a_panel_as_tall_as_the_window() {
    let tree = open_drawer_in(
        (400.0, 300.0),
        tall_body(40),
        DrawerProps::props().control(signal(true)).build(),
    );
    let rect = panel(&tree).0;
    assert!(rect.y >= 0.0 && rect.y + rect.height <= 300.0, "{rect:?}");

    let stops = telar::focus::exposed()
        .into_iter()
        .filter(|exposed| exposed.role == Role::ScrollArea)
        .filter_map(|exposed| telar::visible_rect(exposed.node))
        .collect::<Vec<_>>();
    assert_eq!(
        stops.len(),
        1,
        "an overflowing body is a stop the keyboard can scroll"
    );
    assert!(stops[0].y + stops[0].height <= rect.y + rect.height);

    let names: Vec<String> = telar::inspect::snapshot(&tree.commands())
        .into_iter()
        .filter(|node| node.role == Role::ScrollArea)
        .map(|node| node.name)
        .collect();
    assert_eq!(names, ["Drawer"], "named as the panel is");
}
