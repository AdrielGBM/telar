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
    let (mut host, stacks) = shell_stacks();

    stacks.push(SectionRoute::Source);
    host.sync();
    assert_eq!(host.current_route(), Some(SectionRoute::Source));
    assert_eq!(stacks.depth(), 2);

    assert!(stacks.back(), "the pushed detail page is what back walks off");
    host.sync();
    assert_eq!(host.current_route(), Some(SectionRoute::Overview));
    assert!(
        !stacks.back(),
        "at a section's root with nothing open there is nothing to go back to"
    );
}

#[test]
fn a_section_keeps_its_own_depth_while_you_read_another() {
    let (mut host, stacks) = shell_stacks();
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
    let (mut host, stacks) = shell_stacks();
    stacks.push(SectionRoute::Source);
    host.sync();

    stacks.select(5);
    host.sync();
    assert_eq!(host.current_route(), Some(SectionRoute::Overview));
    assert_eq!(stacks.depth(), 1);
}
