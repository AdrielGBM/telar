use super::*;

// Every entry's `.rsx` file must exist and carry a view — the macro table pairs a builder with a file name by hand, so a renamed or moved feature would otherwise bake in the wrong source.
#[test]
fn every_section_bakes_the_source_of_its_own_feature() {
    for def in SECTIONS {
        assert!(def.file.ends_with(".rsx"), "{}: {}", def.title, def.file);
        assert!(
            def.source.contains("[view]"),
            "{} baked no view from {}",
            def.title,
            def.file
        );
    }
}

fn shell_stacks() -> (TabHost<usize, SectionRoute>, TabStacks<usize, SectionRoute>) {
    reset_layout_runtime();
    telar::set_theme(crate::core::theme::SandboxTheme::modern());
    let sections: Vec<usize> = (0..SECTIONS.len()).collect();
    let stacks = TabStacks::new(signal(5usize), &sections, |_| {
        Navigator::new(SectionRoute::Overview)
    });
    let factory = stacks.clone();
    let host = TabHost::new(
        stacks.clone(),
        move |section: &usize, route: &SectionRoute| build_page(&factory, *section, *route),
    )
    .unwrap()
    .with_policy(PagePolicy::Transient);
    (host, stacks)
}

#[test]
fn source_detail_pushes_a_page_and_back_returns_to_the_section() {
    let (host, stacks) = shell_stacks();

    stacks.push(SectionRoute::Source);
    host.sync();
    assert_eq!(host.current_route(), Some(SectionRoute::Source));
    assert_eq!(stacks.depth(), 2);

    assert!(
        stacks.back(),
        "the pushed detail page is what back walks off"
    );
    host.sync();
    assert_eq!(host.current_route(), Some(SectionRoute::Overview));
    assert!(
        !stacks.back(),
        "at a section's root with nothing open there is nothing to go back to"
    );
}

#[test]
fn a_section_keeps_its_own_depth_while_you_read_another() {
    let (host, stacks) = shell_stacks();
    stacks.push(SectionRoute::Source);
    host.sync();

    stacks.select(9);
    host.sync();
    assert_eq!(host.current_tab(), 9);
    assert_eq!(
        host.current_route(),
        Some(SectionRoute::Overview),
        "arriving at a section lands on its overview, not the depth of the one you left"
    );

    stacks.select(5);
    host.sync();
    assert_eq!(
        host.current_route(),
        Some(SectionRoute::Source),
        "the section you left is still showing its source listing"
    );
}

#[test]
fn reselecting_the_current_section_returns_to_its_overview() {
    let (host, stacks) = shell_stacks();
    stacks.push(SectionRoute::Source);
    host.sync();

    stacks.select(5);
    host.sync();
    assert_eq!(host.current_route(), Some(SectionRoute::Overview));
    assert_eq!(stacks.depth(), 1);
}

/// The links in the bar marked current, by where they go.
fn current_links(tree: &telar::ComponentList) -> Vec<telar::Destination> {
    tree.commands()
        .iter()
        .filter_map(|command| match command {
            telar::DrawCommand::PushElement { element } => element
                .semantics
                .role
                .link()
                .filter(|link| link.current)
                .and_then(|link| link.destination.clone()),
            _ => None,
        })
        .collect()
}

/// The demo's section is built inside the scroll it is shown in, so its bar reads that scroll: following the link to a section brings it under the bar, and the link to it becomes the current one.
#[test]
fn the_current_place_demo_marks_the_link_to_the_section_under_its_bar() {
    reset_layout_runtime();
    telar::set_theme(crate::core::theme::SandboxTheme::modern());
    telar::receive_location_history(vec![telar::Location::root()]);
    let section = SECTIONS
        .iter()
        .position(|def| def.title == "Current place")
        .expect("the demo is listed");
    let page = SectionPage::new(Navigator::new(SectionRoute::Overview), section).unwrap();
    compute_layout(
        page.layout_node(),
        AvailableSpace::Definite(900.0),
        AvailableSpace::Definite(700.0),
    )
    .unwrap();
    let mut tree = telar::ComponentList::new(page);
    tree.on_event(&Event::WindowResized {
        width: 900,
        height: 700,
    });
    telar::relayout_if_dirty();
    assert_eq!(current_links(&tree), [], "the header is under the bar");

    assert!(telar::follow(&telar::anchor("place-usage")));
    telar::relayout_if_dirty();
    assert_eq!(current_links(&tree), [telar::anchor("place-usage")]);
}

fn tap(tree: &mut telar::ComponentList, node: &telar::AccessNode) {
    use platform_core::{PointerButton, PointerSource};
    let (x, y) = crate::nav::center(node);
    tree.on_event(&Event::PointerPressed {
        x,
        y,
        button: PointerButton::Primary,
        source: PointerSource::Touch { id: 0 },
    });
    tree.on_event(&Event::PointerReleased {
        x,
        y,
        button: PointerButton::Primary,
        source: PointerSource::Touch { id: 0 },
    });
}

fn menu_button(nodes: &[telar::AccessNode]) -> &telar::AccessNode {
    nodes
        .iter()
        .find(|node| node.role == Role::Button && node.name == "Menu")
        .expect("a phone shows a Menu button")
}

fn sections_read(nodes: &[telar::AccessNode]) -> usize {
    SECTIONS
        .iter()
        .filter(|def| {
            nodes
                .iter()
                .any(|node| node.role == Role::Button && node.name == def.title)
        })
        .count()
}

fn every_button_is_named(nodes: &[telar::AccessNode]) -> bool {
    nodes
        .iter()
        .filter(|node| node.role == Role::Button)
        .all(|node| !node.name.is_empty())
}

/// On a phone the rail is a drawer the shell draws only while it is open: shut, a reader is told the page and the Menu button and nothing of the drawer, so no control of it takes the name of the page's words drawn where it would be; open, every section in it is read by its own name.
#[test]
fn a_phone_reads_the_drawer_only_while_it_is_open() {
    telar::set_theme(crate::core::theme::SandboxTheme::modern());
    let mut tree = telar::ComponentList::new(SandboxRoot.root());
    tree.on_event(&Event::WindowResized {
        width: 393,
        height: 800,
    });

    let shut = telar::inspect::snapshot(&tree.commands());
    assert_eq!(menu_button(&shut).expanded, Some(false));
    assert_eq!(sections_read(&shut), 0, "{shut:#?}");
    assert!(every_button_is_named(&shut), "{shut:#?}");
    assert!(shut.iter().any(|node| node.role == Role::Main));

    tap(&mut tree, menu_button(&shut));
    let open = telar::inspect::snapshot(&tree.commands());
    assert_eq!(menu_button(&open).expanded, Some(true));
    assert_eq!(sections_read(&open), SECTIONS.len(), "{open:#?}");
    assert!(every_button_is_named(&open), "{open:#?}");
}

/// A reader activating a section in the open drawer sends no event the shell could act on, yet the next frame shows that section and the drawer shut, as a tap on it does.
#[test]
fn a_section_a_reader_activates_in_the_drawer_shows_and_shuts_the_drawer() {
    telar::set_theme(crate::core::theme::SandboxTheme::modern());
    let mut tree = telar::ComponentList::new(SandboxRoot.root());
    tree.on_event(&Event::WindowResized {
        width: 393,
        height: 800,
    });
    let shut = telar::inspect::snapshot(&tree.commands());
    tap(&mut tree, menu_button(&shut));
    let open = telar::inspect::snapshot(&tree.commands());
    let showing = |nodes: &[telar::AccessNode], def: &SectionDef| {
        let source = format!("View source {}", def.file);
        nodes
            .iter()
            .any(|node| node.role == Role::Button && node.name == source)
    };
    let chosen = SECTIONS
        .iter()
        .find(|def| !showing(&open, def))
        .expect("a section other than the one on screen");
    let item = open
        .iter()
        .find(|node| node.role == Role::Button && node.name == chosen.title)
        .and_then(|node| node.id)
        .expect("the drawer offers the section");

    assert!(telar::focus::perform(
        item.id,
        telar::focus::AccessAction::Activate
    ));
    telar::relayout_if_dirty();

    let after = telar::inspect::snapshot(&tree.commands());
    assert!(showing(&after, chosen), "{after:#?}");
    assert_eq!(menu_button(&after).expanded, Some(false));
    assert_eq!(sections_read(&after), 0, "{after:#?}");
}

/// Whether every button a reader is told of starts below a status bar `top` tall, where a tap reaches it rather than the bar.
fn every_button_is_below(nodes: &[telar::AccessNode], top: f32) -> bool {
    nodes
        .iter()
        .filter(|node| node.role == Role::Button)
        .all(|node| node.rect.y >= top)
}

/// On a phone the system draws its status bar over the top of the window and its navigation bar over the bottom; the Menu button and the drawer's sections start below the status bar, and follow it when it changes.
#[test]
fn a_phone_keeps_the_shell_clear_of_the_system_bars() {
    telar::set_theme(crate::core::theme::SandboxTheme::modern());
    let bars = Insets::new(40.0, 0.0, 24.0, 0.0);
    telar::set_safe_area_insets(Insets::default());
    let mut tree = telar::ComponentList::new(SandboxRoot.root());
    tree.on_event(&Event::WindowResized {
        width: 393,
        height: 800,
    });
    telar::set_safe_area_insets(bars);
    telar::relayout_if_dirty();

    let shut = telar::inspect::snapshot(&tree.commands());
    assert!(every_button_is_below(&shut, bars.top), "{shut:#?}");

    tap(&mut tree, menu_button(&shut));
    let open = telar::inspect::snapshot(&tree.commands());
    assert_eq!(sections_read(&open), SECTIONS.len(), "{open:#?}");
    assert!(every_button_is_below(&open, bars.top), "{open:#?}");
}

/// Every control a section shows has a name for a reader to say, so none is announced as a bare "slider" or "edit box".
#[test]
fn every_control_of_every_section_is_named() {
    for (section, def) in SECTIONS.iter().enumerate() {
        reset_layout_runtime();
        telar::set_theme(crate::core::theme::SandboxTheme::modern());
        let page = SectionPage::new(Navigator::new(SectionRoute::Overview), section).unwrap();
        compute_layout(
            page.layout_node(),
            AvailableSpace::Definite(900.0),
            AvailableSpace::Definite(700.0),
        )
        .unwrap();
        let tree = telar::ComponentList::new(page);
        telar::relayout_if_dirty();
        let unnamed: Vec<_> = telar::inspect::snapshot(&tree.commands())
            .into_iter()
            .filter(|node| node.id.is_some() && node.name.is_empty())
            .map(|node| node.role)
            .collect();
        assert!(unnamed.is_empty(), "{}: {unnamed:?}", def.title);
    }
}
