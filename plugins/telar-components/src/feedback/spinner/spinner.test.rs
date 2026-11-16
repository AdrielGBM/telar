use telar::{
    Accessible, AlignItems, Children, Color, ComponentList, Container, ControlSize, DrawCommand,
    LayoutItem, LayoutStyle, Rect, Role, Text, Tone, box_item,
};

use crate::{SpinnerProps, spinner};

fn shown(props: SpinnerProps) -> telar::testing::Scoped<(Rect, ComponentList)> {
    crate::test_support::fresh_layout_runtime();
    telar::testing::scoped(|| {
        let widget = spinner(props).unwrap();
        let node = widget.layout_node();
        let row = Container::new(
            LayoutStyle::row().align_items(AlignItems::CENTER),
            vec![widget],
        )
        .unwrap();
        telar::testing::lay_out(row.layout_node(), 100.0, 100.0);
        let rect = telar::visible_rect(node).expect("the spinner is laid out");
        (rect, ComponentList::new(row))
    })
}

fn stroke(tree: &ComponentList) -> Option<Color> {
    tree.commands().iter().find_map(|command| match command {
        DrawCommand::Path { style, .. } => style
            .stroke
            .as_ref()
            .map(|stroke| stroke.paint.solid_color()),
        _ => None,
    })
}

#[test]
fn a_spinner_stands_in_an_icons_box_at_every_size() {
    for size in [ControlSize::Small, ControlSize::Regular, ControlSize::Large] {
        let rect = shown(SpinnerProps::props().size(size).build()).0;
        let side = crate::metrics::icon_size(size);
        assert_eq!((rect.width, rect.height), (side, side), "{size:?}");
    }
}

#[test]
fn a_named_tone_colours_the_arc_with_the_themes_token() {
    let named = stroke(&shown(SpinnerProps::props().tone(Tone::Danger).build()).1);
    assert_eq!(named, Some(telar::use_theme_tokens().error()));

    let plain = stroke(&shown(SpinnerProps::props().build()).1);
    assert!(plain.is_some(), "the arc is drawn without a tone");
    assert_ne!(plain, named, "and takes the ink of its place");
}

#[test]
fn what_the_call_nests_turns_in_place_of_the_arc_and_sizes_the_spinner() {
    let nested = Children::one(|| {
        Ok(box_item(Text::declaring(
            || "*".to_string(),
            LayoutStyle::new().width(48.0).height(48.0),
            |inherited| inherited,
        )?))
    });
    let shown = shown(SpinnerProps::props().children(nested).build());
    let (rect, tree) = (shown.0, &shown.1);
    assert_eq!((rect.width, rect.height), (48.0, 48.0));
    assert!(crate::test_support::find_text(&tree.commands(), "*"));
    assert_eq!(stroke(tree), None, "no arc beside it");
}

#[test]
fn a_spinner_turns_as_time_passes() {
    let tree = &shown(SpinnerProps::props().build()).1;
    let start = web_time::Instant::now();
    telar::motion::tick(start);
    let before = format!("{:?}", tree.commands());
    telar::motion::tick(start + std::time::Duration::from_millis(225));
    assert_ne!(format!("{:?}", tree.commands()), before);
}

/// A reader hears a spinner as a progress bar with no reading, which says the work is under way and not how far it has got, under the name its call gave it.
#[test]
fn a_reader_hears_a_named_progress_bar_with_no_reading() {
    crate::test_support::fresh_layout_runtime();
    let tree = telar::testing::scoped(|| {
        let widget = spinner(SpinnerProps::props().build())
            .unwrap()
            .a11y_label(|| "Saving");
        telar::testing::lay_out(widget.layout_node(), 100.0, 100.0);
        ComponentList::new(widget)
    });
    let node = telar::inspect::snapshot(&tree.commands())
        .into_iter()
        .find(|node| node.role == Role::ProgressBar)
        .expect("the spinner is read");
    assert_eq!(node.name, "Saving");
    assert_eq!(node.value, None, "indeterminate");
    assert_eq!(node.id, None, "no keyboard stop");
}
