use telar::testing::lay_out;
use telar::{AccessNode, ComponentList, DrawCommand, Role, relayout_if_dirty, signal};

use super::*;

fn pixels(width: u32, height: u32) -> Arc<ImageData> {
    Arc::new(ImageData::new(
        vec![200; (width * height * 4) as usize],
        width,
        height,
    ))
}

fn mounted(props: ImageProps) -> ComponentList {
    crate::test_support::fresh_layout_runtime();
    let item = image(props).unwrap();
    lay_out(item.layout_node(), 400.0, 400.0);
    let tree = ComponentList::new(item);
    let _ = tree.commands();
    tree
}

fn settle(tree: &ComponentList) {
    relayout_if_dirty();
    let _ = tree.commands();
}

fn drawn_image(tree: &ComponentList) -> Option<telar::Rect> {
    tree.commands().iter().find_map(|command| match command {
        DrawCommand::Image { rect, .. } => Some(*rect),
        _ => None,
    })
}

fn read(tree: &ComponentList) -> Vec<AccessNode> {
    telar::inspect::snapshot(&tree.commands())
}

fn texts(tree: &ComponentList) -> Vec<String> {
    telar::testing::texts(tree)
}

#[test]
fn a_picture_is_drawn_and_named_by_its_words() {
    let tree = mounted(
        ImageProps::props()
            .source(ImageSource::from(pixels(40, 20)))
            .alt("A red kite over the hills")
            .build(),
    );
    let drawn = drawn_image(&tree).expect("the picture is drawn");
    assert_eq!((drawn.width, drawn.height), (40.0, 20.0));
    let picture = read(&tree)
        .into_iter()
        .find(|node| node.role == Role::Drawing)
        .expect("a reader meets the picture");
    assert_eq!(picture.name, "A red kite over the hills");
}

#[test]
fn a_picture_with_no_words_only_decorates_and_a_reader_steps_over_it() {
    let tree = mounted(
        ImageProps::props()
            .source(ImageSource::from(pixels(8, 8)))
            .alt("")
            .build(),
    );
    assert!(drawn_image(&tree).is_some());
    assert!(
        read(&tree).iter().all(|node| node.role != Role::Drawing),
        "{:?}",
        read(&tree)
    );
}

#[test]
fn one_side_given_takes_the_other_from_the_picture() {
    let tree = mounted(
        ImageProps::props()
            .source(ImageSource::from(pixels(40, 20)))
            .alt("Wide")
            .width(120.0)
            .fit(ObjectFit::Cover)
            .build(),
    );
    let drawn = drawn_image(&tree).expect("drawn");
    assert_eq!((drawn.width, drawn.height), (120.0, 60.0));
}

#[test]
fn a_loading_picture_stands_in_as_a_quiet_box_of_its_size_until_it_arrives() {
    let source = signal(ImageSource::Loading);
    let tree = mounted(
        ImageProps::props()
            .source(source)
            .alt("Avatar")
            .width(48.0)
            .height(48.0)
            .build(),
    );
    assert!(drawn_image(&tree).is_none());
    let stand_in = crate::test_support::first_painted(&tree);
    assert_eq!(
        stand_in.fill.map(|p| p.solid_color()),
        Some(shared::surface_alt())
    );
    assert!(texts(&tree).is_empty(), "nothing is said while it loads");

    source.set(ImageSource::Ready(pixels(10, 10)));
    settle(&tree);
    let drawn = drawn_image(&tree).expect("the picture replaces its stand-in");
    assert_eq!((drawn.width, drawn.height), (48.0, 48.0));
}

#[test]
fn a_picture_that_could_not_be_had_shows_its_words_instead() {
    let tree = mounted(
        ImageProps::props()
            .source(ImageSource::Failed)
            .alt("Chart of sales")
            .width(160.0)
            .height(90.0)
            .build(),
    );
    assert!(drawn_image(&tree).is_none());
    assert!(texts(&tree).contains(&"Chart of sales".to_string()));
    let named: Vec<String> = read(&tree).into_iter().map(|node| node.name).collect();
    assert_eq!(
        named
            .iter()
            .filter(|name| *name == "Chart of sales")
            .count(),
        1,
        "a reader hears the words once: {named:?}"
    );
}

#[test]
fn the_corner_follows_the_role_until_the_tokens_name_one() {
    crate::test_support::assert_corner_follows_the_role_until_the_tokens_name_one(
        || {
            mounted(
                ImageProps::props()
                    .source(ImageSource::Loading)
                    .alt("Cover")
                    .width(32.0)
                    .height(32.0)
                    .build(),
            )
        },
        |tree| {
            settle(tree);
            crate::test_support::first_painted(tree).radius
        },
        |_, role| BorderRadius::all(role),
        |radius| ImageTokens {
            radius: Some(radius),
        },
    );
}
