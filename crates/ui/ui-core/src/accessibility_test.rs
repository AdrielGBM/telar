use std::sync::Arc;

use layout_core::NodeId;
use layout_core::{AvailableSpace, LayoutStyle};
use renderer_core::{RectStyle, TextStyle};

use super::*;
use crate::container::Container;
use crate::context::{compute_layout, reset_layout_runtime};
use crate::layout_item::{LayoutItem, box_item};
use crate::styled_container::StyledContainer;
use crate::text::Text;

fn text_at(text: &str, rect: Rect) -> DrawCommand {
    DrawCommand::Text {
        spans: None,
        text: Arc::from(text),
        rect,
        style: Arc::new(TextStyle::new(12.0, renderer_core::Color::BLACK)),
    }
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::new(x, y, w, h)
}

/// A button built the ordinary way is announced with the label it draws — nobody wrote an accessible name for it, and that is the point: a second copy of the text is a second thing to keep true.
#[test]
fn a_control_is_named_by_the_text_it_draws() {
    reset_layout_runtime();
    focus::clear();
    let label = Text::new(
        || "Save".to_string(),
        LayoutStyle::new().width(40.0).height(16.0),
        || TextStyle::new(12.0, renderer_core::Color::BLACK),
    )
    .unwrap();
    let button = StyledContainer::new(
        LayoutStyle::new().width(80.0).height(30.0),
        |_r| RectStyle::default(),
        vec![box_item(label)],
    )
    .unwrap()
    .control(Role::Button)
    .on_press(|| {});
    let root = Container::new(LayoutStyle::new().flex_column(), vec![box_item(button)]).unwrap();
    compute_layout(
        root.layout_node(),
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();

    let nodes = snapshot(&[text_at("Save", rect(10.0, 5.0, 40.0, 16.0))]);
    let button = nodes
        .iter()
        .find(|n| n.id.is_some())
        .expect("the button is exposed");
    assert_eq!(button.name, "Save");
    assert_eq!(button.role, Role::Button);
    assert!(
        button.enabled,
        "a control is exposed enabled unless it says otherwise"
    );
}

/// Text belonging to no control is content, not noise. A reader handed only the buttons cannot say what the buttons are for.
#[test]
fn text_outside_any_control_is_still_announced() {
    reset_layout_runtime();
    focus::clear();
    let nodes = snapshot(&[text_at("Delete everything?", rect(0.0, 0.0, 200.0, 20.0))]);
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].role, Role::Label);
    assert_eq!(nodes[0].name, "Delete everything?");
    assert_eq!(nodes[0].id, None);
}

/// Nesting: the label goes to the smallest control that contains it, so a button inside a card is named by its own text rather than by everything the card happens to hold.
#[test]
fn a_label_belongs_to_the_smallest_control_around_it() {
    let outer = rect(0.0, 0.0, 200.0, 100.0);
    let inner = rect(10.0, 10.0, 50.0, 20.0);
    assert!(
        contains(outer, inner),
        "the outer box encloses the inner one"
    );
    assert!(
        contains(inner, rect(20.0, 15.0, 10.0, 8.0)),
        "and the inner one encloses the label"
    );
    assert!(
        !contains(inner, rect(120.0, 15.0, 10.0, 8.0)),
        "a label to the side of it is not inside"
    );
    assert!(
        area(inner) < area(outer),
        "the inner box is the smaller of the two, so the label is its"
    );
}

/// A value control that reports only its role has not said the one thing it exists to report: a slider announced as "Volume, slider" leaves the number — the whole content of the control — unsaid.
#[test]
fn a_valued_control_reports_where_it_stands() {
    reset_layout_runtime();
    focus::clear();
    let track = StyledContainer::new(
        LayoutStyle::new().width(200.0).height(20.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .control(Role::Slider)
    .valued(|| platform_core::NumericValue {
        now: 0.25,
        min: 0.0,
        max: 1.0,
    });
    let root = Container::new(LayoutStyle::new().flex_column(), vec![box_item(track)]).unwrap();
    compute_layout(
        root.layout_node(),
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();

    let nodes = snapshot(&[]);
    let slider = nodes.iter().find(|n| n.id.is_some()).expect("exposed");
    let value = slider.value.expect("a slider carries a number");
    assert_eq!(value.now, 0.25);
    assert_eq!((value.min, value.max), (0.0, 1.0));
}

/// `toggled` was populated only through `labelled_control`, so a tab never said it was the selected one.
#[test]
fn a_control_that_carries_a_state_reports_it() {
    reset_layout_runtime();
    focus::clear();
    let tab = StyledContainer::new(
        LayoutStyle::new().width(80.0).height(30.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .control(Role::Tab)
    .toggled(|| true);
    let root = Container::new(LayoutStyle::new().flex_column(), vec![box_item(tab)]).unwrap();
    compute_layout(
        root.layout_node(),
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();

    let nodes = snapshot(&[]);
    let tab = nodes.iter().find(|n| n.id.is_some()).expect("exposed");
    assert_eq!(tab.toggled, Some(true));
}

/// The reading order a reader walks is the order things sit on screen, not the order they were built — which is what tab order is, and stays.
#[test]
fn nodes_come_back_in_reading_order() {
    reset_layout_runtime();
    focus::clear();
    let nodes = snapshot(&[
        text_at("second", rect(0.0, 40.0, 50.0, 16.0)),
        text_at("first", rect(0.0, 10.0, 50.0, 16.0)),
        text_at("third", rect(60.0, 40.0, 50.0, 16.0)),
    ]);
    let names: Vec<&str> = nodes.iter().map(|n| n.name.as_str()).collect();
    assert_eq!(names, vec!["first", "second", "third"]);
}

fn open(node: NodeId, bounds: Rect) -> DrawCommand {
    DrawCommand::PushElement {
        element: Arc::new(renderer_core::Element::new(
            renderer_core::ElementId(node.into()),
            renderer_core::Semantics::group(),
            "",
            bounds,
        )),
    }
}

fn letter(c: &'static str) -> Text {
    Text::new(
        move || c.to_string(),
        LayoutStyle::new().width(10.0).height(16.0),
        || TextStyle::new(12.0, renderer_core::Color::BLACK),
    )
    .unwrap()
}

/// Split letters: the row is read as the word it spells, once, and none of the letters is read on its own.
#[test]
fn a_named_box_over_hidden_letters_reads_as_one_word() {
    use crate::annotation::Accessible;
    reset_layout_runtime();
    focus::clear();
    let h = letter("H").a11y_hidden();
    let i = letter("i").a11y_hidden();
    let (h_node, i_node) = (h.layout_node(), i.layout_node());
    let word = Container::new(
        LayoutStyle::new().flex_row(),
        vec![box_item(h), box_item(i)],
    )
    .unwrap()
    .a11y_label(|| "Hi")
    .a11y_lang(|| "en");
    compute_layout(
        word.layout_node(),
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();

    let frame = [
        open(word.layout_node(), rect(0.0, 0.0, 20.0, 16.0)),
        open(h_node, rect(0.0, 0.0, 10.0, 16.0)),
        text_at("H", rect(0.0, 0.0, 10.0, 16.0)),
        DrawCommand::PopElement,
        open(i_node, rect(10.0, 0.0, 10.0, 16.0)),
        text_at("i", rect(10.0, 0.0, 10.0, 16.0)),
        DrawCommand::PopElement,
        DrawCommand::PopElement,
    ];
    let nodes = snapshot(&frame);
    assert_eq!(nodes.len(), 1, "one word, not a word and its letters");
    assert_eq!(nodes[0].name, "Hi");
    assert_eq!(nodes[0].role, Role::Label);
    assert_eq!(nodes[0].lang.as_deref(), Some("en"));
    assert_eq!(platform_core::accessibility::transcript(&nodes), "Hi");
}

/// A language reaches everything under the box that said it, and a nearer one wins.
#[test]
fn a_language_reaches_the_text_beneath_it() {
    use crate::annotation::Accessible;
    reset_layout_runtime();
    focus::clear();
    let quote = letter("Hola").a11y_lang(|| "es");
    let quote_node = quote.layout_node();
    let page = Container::new(LayoutStyle::new(), vec![box_item(quote)])
        .unwrap()
        .a11y_lang(|| "en");
    let frame = [
        open(page.layout_node(), rect(0.0, 0.0, 100.0, 40.0)),
        text_at("Hello", rect(0.0, 0.0, 40.0, 16.0)),
        open(quote_node, rect(0.0, 20.0, 40.0, 16.0)),
        text_at("Hola", rect(0.0, 20.0, 40.0, 16.0)),
        DrawCommand::PopElement,
        DrawCommand::PopElement,
    ];
    let nodes = snapshot(&frame);
    let lang_of = |name: &str| {
        nodes
            .iter()
            .find(|n| n.name == name)
            .and_then(|n| n.lang.clone())
    };
    assert_eq!(lang_of("Hello").as_deref(), Some("en"));
    assert_eq!(lang_of("Hola").as_deref(), Some("es"));
}

/// A control the application named is announced by that name, and not by the glyph it draws.
#[test]
fn a_named_control_is_not_renamed_by_its_icon() {
    use crate::annotation::Accessible;
    reset_layout_runtime();
    focus::clear();
    let close = StyledContainer::new(
        LayoutStyle::new().width(30.0).height(30.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .control(Role::Button)
    .on_press(|| {})
    .a11y_label(|| "Close");
    let node = close.layout_node();
    let root = Container::new(LayoutStyle::new().flex_column(), vec![box_item(close)]).unwrap();
    compute_layout(
        root.layout_node(),
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();

    let nodes = snapshot(&[
        open(node, rect(0.0, 0.0, 30.0, 30.0)),
        text_at("×", rect(5.0, 5.0, 20.0, 20.0)),
        DrawCommand::PopElement,
    ]);
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].name, "Close");
    assert_eq!(nodes[0].role, Role::Button);
    assert_eq!(
        platform_core::accessibility::transcript(&nodes),
        "Close, button"
    );
}

/// A control inside a hidden box is not offered to a reader either: hiding takes the subtree.
#[test]
fn a_hidden_box_takes_its_controls_with_it() {
    use crate::annotation::Accessible;
    reset_layout_runtime();
    focus::clear();
    let button = StyledContainer::new(
        LayoutStyle::new().width(30.0).height(30.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .control(Role::Button)
    .on_press(|| {});
    let button_node = button.layout_node();
    let decoration = Container::new(LayoutStyle::new(), vec![box_item(button)])
        .unwrap()
        .a11y_hidden();
    compute_layout(
        decoration.layout_node(),
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();

    let nodes = snapshot(&[
        open(decoration.layout_node(), rect(0.0, 0.0, 30.0, 30.0)),
        open(button_node, rect(0.0, 0.0, 30.0, 30.0)),
        text_at("Go", rect(0.0, 0.0, 30.0, 30.0)),
        DrawCommand::PopElement,
        DrawCommand::PopElement,
    ]);
    assert!(
        nodes.is_empty(),
        "nothing under a hidden box is read: {nodes:?}"
    );
}

/// A picture is read only when it is named, and then as an image.
#[test]
fn a_named_picture_is_read_as_an_image() {
    use crate::annotation::Accessible;
    reset_layout_runtime();
    focus::clear();
    let logo = letter("").a11y_label(|| "Company logo");
    let node = logo.layout_node();
    compute_layout(
        node,
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();
    let picture = DrawCommand::Image {
        data: Arc::new(renderer_core::ImageData::new(vec![0; 4], 1, 1)),
        rect: rect(0.0, 0.0, 10.0, 16.0),
        raster: renderer_core::Raster::Smooth,
        fill: renderer_core::ImageFill::Stretch,
    };
    let nodes = snapshot(&[
        open(node, rect(0.0, 0.0, 10.0, 16.0)),
        picture,
        DrawCommand::PopElement,
    ]);
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].role, Role::Drawing);
    assert_eq!(
        platform_core::accessibility::transcript(&nodes),
        "Company logo, image"
    );
}

/// A link run inside a paragraph is announced as a link of its own, named by its words and carrying where it goes, after the paragraph that holds it.
#[test]
fn a_link_run_is_a_link_after_its_paragraph() {
    reset_layout_runtime();
    let spans: Arc<[renderer_core::Span]> = Arc::from(vec![
        renderer_core::Span::new(9..13, renderer_core::Declared::default())
            .linking_to(platform_core::Destination::external("https://example.com").unwrap()),
    ]);
    let nodes = snapshot(&[DrawCommand::Text {
        spans: Some(spans),
        text: Arc::from("Meet the team today"),
        rect: rect(0.0, 0.0, 200.0, 20.0),
        style: Arc::new(TextStyle::new(12.0, renderer_core::Color::BLACK)),
    }]);
    let described: Vec<(Role, &str, Option<&str>)> = nodes
        .iter()
        .map(|node| (node.role, node.name.as_str(), node.url.as_deref()))
        .collect();
    assert_eq!(
        described,
        [
            (Role::Label, "Meet the team today", None),
            (Role::Link, "team", Some("https://example.com")),
        ]
    );
}

/// A raster target draws a label in its own box's space and moves it into place with a matrix, so the label is read where the matrix puts it. Read where it was drawn, every label sat at the window's corner and no control was named by its text.
#[test]
fn a_label_moved_into_place_by_a_matrix_names_its_control() {
    reset_layout_runtime();
    focus::clear();
    let pad = Container::new(LayoutStyle::new().height(100.0), vec![]).unwrap();
    let button = StyledContainer::new(
        LayoutStyle::new().width(80.0).height(30.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .control(Role::Button)
    .on_press(|| {});
    let root = Container::new(
        LayoutStyle::new().flex_column(),
        vec![box_item(pad), box_item(button)],
    )
    .unwrap();
    compute_layout(
        root.layout_node(),
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();

    let frame = [
        DrawCommand::PushMatrix {
            matrix: [1.0, 0.0, 0.0, 1.0, 10.0, 105.0],
        },
        text_at("Save", rect(0.0, 0.0, 40.0, 16.0)),
        DrawCommand::PopMatrix,
    ];
    assert!(frame.iter().all(is_read), "the runner keeps the matrices");
    let nodes = snapshot(&frame);
    let button = nodes
        .iter()
        .find(|n| n.id.is_some())
        .expect("the button is exposed");
    assert_eq!(button.name, "Save");
}

/// A tree keeps focus while its cursor walks the rows, so the snapshot names the row the cursor is on; a cursor on something the snapshot does not carry names nothing.
#[test]
fn a_focused_tree_names_the_row_its_cursor_rests_on() {
    reset_layout_runtime();
    focus::clear();
    let row = || {
        StyledContainer::new(
            LayoutStyle::new().width(100.0).height(20.0),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .presented(Role::TreeItem)
    };
    let (first, second) = (row(), row());
    let rows = [first.focus_id().unwrap(), second.focus_id().unwrap()];
    let elsewhere = focus::next_id();
    focus::register_as(elsewhere, focus::FocusKind::Widget);
    let cursor = reactive_core::signal(rows[1]);
    let tree = StyledContainer::new(
        LayoutStyle::new().flex_column(),
        |_r| RectStyle::default(),
        vec![box_item(first), box_item(second)],
    )
    .unwrap()
    .control(Role::Tree)
    .active_descendant(move || Some(cursor.get()));
    let tree_id = tree.focus_id().unwrap();
    let root = Container::new(LayoutStyle::new().flex_column(), vec![box_item(tree)]).unwrap();
    compute_layout(
        root.layout_node(),
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();
    focus::request(tree_id);

    let tree_node = |nodes: &[AccessNode]| {
        nodes
            .iter()
            .find(|n| n.role == Role::Tree)
            .cloned()
            .expect("the tree is exposed")
    };
    let nodes = snapshot(&[]);
    let said = tree_node(&nodes);
    assert!(said.focused, "focus stays on the tree");
    assert_eq!(said.active_descendant, Some(rows[1]));
    let read = platform_core::accessibility::transcript(&nodes);
    assert!(
        read.ends_with(", treeitem, focused") && !read.contains("tree, focused"),
        "a reader says the row is focused, not the tree: {read:?}"
    );

    cursor.set(elsewhere);
    assert_eq!(tree_node(&snapshot(&[])).active_descendant, None);
    focus::clear();
}

/// A scroll area lays its content out as a root of its own and moves it by the scroll offset, so a control inside one is reported where it is drawn: in window space, the same space its label is read in.
#[test]
fn a_control_inside_a_scrolled_area_is_named_where_it_is_drawn() {
    use crate::scroll_area::LayoutScrollArea;
    reset_layout_runtime();
    focus::clear();
    let label = Text::new(
        || "Save".to_string(),
        LayoutStyle::new().width(40.0).height(16.0),
        || TextStyle::new(12.0, renderer_core::Color::BLACK),
    )
    .unwrap();
    let button = StyledContainer::new(
        LayoutStyle::new().width(80.0).height(30.0),
        |_r| RectStyle::default(),
        vec![box_item(label)],
    )
    .unwrap()
    .control(Role::Button)
    .on_press(|| {});
    let spacer = Container::new(LayoutStyle::new().height(300.0), vec![]).unwrap();
    let content = Container::new(
        LayoutStyle::new().flex_column(),
        vec![box_item(spacer), box_item(button)],
    )
    .unwrap();
    let (offset_x, offset_y) = (reactive_core::signal(0.0f32), reactive_core::signal(0.0f32));
    let scroll = LayoutScrollArea::new_keeping(
        LayoutStyle::new().width(200.0).height(100.0),
        (offset_x, offset_y),
        |_| Ok(box_item(content)),
    )
    .unwrap();
    let header = Container::new(LayoutStyle::new().height(50.0), vec![]).unwrap();
    let root = Container::new(
        LayoutStyle::new().flex_column().padding_left(20.0),
        vec![box_item(header), box_item(scroll)],
    )
    .unwrap();
    compute_layout(
        root.layout_node(),
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    offset_y.set(250.0);
    let tree = ui_tree::ComponentList::new(root);

    let nodes = snapshot(&tree.commands());
    let button = nodes
        .iter()
        .find(|n| n.id.is_some())
        .expect("the button is exposed");
    assert_eq!(
        button.name, "Save",
        "the label it draws is inside it on screen"
    );
    assert_eq!(button.rect, rect(20.0, 100.0, 80.0, 30.0));
}

fn drawn_tab(caption: &'static str) -> StyledContainer {
    let words = Text::new(
        move || caption.to_string(),
        LayoutStyle::new().width(50.0).height(20.0),
        || TextStyle::new(12.0, renderer_core::Color::BLACK),
    )
    .unwrap();
    StyledContainer::new(
        LayoutStyle::new().padding_all(5.0),
        |_r| RectStyle::default(),
        vec![box_item(words)],
    )
    .unwrap()
    .control(Role::Tab)
}

fn snapshot_of(root: Container) -> Vec<AccessNode> {
    compute_layout(
        root.layout_node(),
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();
    snapshot(&ui_tree::ComponentList::new(root).commands())
}

fn names_of(nodes: &[AccessNode], role: Role) -> Vec<&str> {
    nodes
        .iter()
        .filter(|n| n.role == role)
        .map(|n| n.name.as_str())
        .collect()
}

fn read_on_its_own(nodes: &[AccessNode], name: &str) -> bool {
    nodes.iter().any(|n| n.id.is_none() && n.name == name)
}

/// A named box with a role of its own names itself and never a control inside it: a tab list of one tab still leaves that tab its own name.
#[test]
fn a_named_group_does_not_rename_the_control_inside_it() {
    use crate::annotation::Accessible;
    reset_layout_runtime();
    focus::clear();
    let list = Container::new(
        LayoutStyle::new().flex_row(),
        vec![box_item(drawn_tab("General"))],
    )
    .unwrap()
    .role(Role::TabList)
    .a11y_label(|| "Sections");
    let root = Container::new(LayoutStyle::new().flex_column(), vec![box_item(list)]).unwrap();

    let nodes = snapshot_of(root);
    assert_eq!(names_of(&nodes, Role::Tab), ["General"]);
    assert!(
        read_on_its_own(&nodes, "Sections"),
        "the group's name is read on its own: {nodes:?}"
    );
}

/// A named box with no role that only wraps a control is that control's name, the way a `<label>` around a field is: a widget built as a box around its control is named by naming the widget.
#[test]
fn a_named_wrapper_names_the_one_control_it_wraps() {
    use crate::annotation::Accessible;
    reset_layout_runtime();
    focus::clear();
    let wrapper = Container::new(
        LayoutStyle::new().flex_row(),
        vec![box_item(drawn_tab("2"))],
    )
    .unwrap()
    .a11y_label(|| "size");
    let root = Container::new(LayoutStyle::new().flex_column(), vec![box_item(wrapper)]).unwrap();

    let nodes = snapshot_of(root);
    assert_eq!(names_of(&nodes, Role::Tab), ["size 2"]);
    assert!(!read_on_its_own(&nodes, "size"), "{nodes:?}");
}

/// A named box with no role that holds more than one control, or words beside its control, is a group of its own: its name is read on its own and renames none of them.
#[test]
fn a_named_box_holding_more_than_a_control_names_itself() {
    use crate::annotation::Accessible;
    reset_layout_runtime();
    focus::clear();
    let pair = Container::new(
        LayoutStyle::new().flex_row(),
        vec![
            box_item(drawn_tab("General")),
            box_item(drawn_tab("Advanced")),
        ],
    )
    .unwrap()
    .a11y_label(|| "Sections");
    let caption = Text::new(
        || "Fixed layer".to_string(),
        LayoutStyle::new().width(80.0).height(20.0),
        || TextStyle::new(12.0, renderer_core::Color::BLACK),
    )
    .unwrap();
    let bar = Container::new(
        LayoutStyle::new().flex_row(),
        vec![box_item(caption), box_item(drawn_tab("Tap"))],
    )
    .unwrap()
    .a11y_label(|| "Fixed bar");
    let root = Container::new(
        LayoutStyle::new().flex_column(),
        vec![box_item(pair), box_item(bar)],
    )
    .unwrap();

    let nodes = snapshot_of(root);
    let mut tabs = names_of(&nodes, Role::Tab);
    tabs.sort();
    assert_eq!(tabs, ["Advanced", "General", "Tap"]);
    assert!(read_on_its_own(&nodes, "Sections"), "{nodes:?}");
    assert!(read_on_its_own(&nodes, "Fixed bar"), "{nodes:?}");
}

fn image_at(rect: Rect) -> DrawCommand {
    DrawCommand::Image {
        data: Arc::new(renderer_core::ImageData::new(vec![0; 4], 1, 1)),
        rect,
        raster: renderer_core::Raster::Smooth,
        fill: renderer_core::ImageFill::Stretch,
    }
}

/// Each box says whether a reader skips it, what it is called and what language it is in, resolved along the frame's nesting: hiding reaches everything inside, and a skipped box is called nothing whatever it was named.
#[test]
fn a_frame_reading_says_what_each_box_is_called_and_whether_it_is_skipped() {
    use crate::annotation::Accessible;
    reset_layout_runtime();
    focus::clear();
    let named = letter("A").a11y_label(|| "Alpha");
    let hidden_named = letter("B").a11y_label(|| "Beta");
    let (named_node, hidden_named_node) = (named.layout_node(), hidden_named.layout_node());
    let hidden = Container::new(LayoutStyle::new(), vec![box_item(hidden_named)])
        .unwrap()
        .a11y_hidden();
    let hidden_node = hidden.layout_node();
    let page = Container::new(
        LayoutStyle::new().flex_column(),
        vec![box_item(named), box_item(hidden)],
    )
    .unwrap()
    .a11y_lang(|| "en");
    let page_node = page.layout_node();

    let reading = FrameReading::of(&[
        open(page_node, rect(0.0, 0.0, 100.0, 40.0)),
        open(named_node, rect(0.0, 0.0, 10.0, 16.0)),
        text_at("A", rect(0.0, 0.0, 10.0, 16.0)),
        DrawCommand::PopElement,
        open(hidden_node, rect(0.0, 20.0, 10.0, 16.0)),
        open(hidden_named_node, rect(0.0, 20.0, 10.0, 16.0)),
        text_at("B", rect(0.0, 20.0, 10.0, 16.0)),
        DrawCommand::PopElement,
        DrawCommand::PopElement,
        DrawCommand::PopElement,
    ]);

    let named = reading.get(named_node).expect("drawn");
    assert!(!named.skipped);
    assert_eq!(named.name.as_deref(), Some("Alpha"));
    assert_eq!(named.lang.as_deref(), Some("en"));
    assert!(reading.skips(hidden_node));
    assert!(reading.skips(hidden_named_node), "hiding takes the subtree");
    assert_eq!(reading.get(hidden_named_node).unwrap().name, None);
    assert!(!reading.skips(page_node));
    assert_eq!(reading.boxes().count(), 4);
    assert!(reading.get(NodeId::from(u64::MAX)).is_none());
}

/// Each picture says whether a reader skips it and what it hears for it: the name of a box that draws only it, nothing under a box that also draws words, and nothing at all once hidden.
#[test]
fn a_frame_reading_says_which_pictures_are_read_and_by_what_name() {
    use crate::annotation::Accessible;
    reset_layout_runtime();
    focus::clear();
    let logo = letter("").a11y_label(|| "Company logo");
    let captioned = letter("").a11y_label(|| "Card");
    let ornament = letter("").a11y_hidden();
    let bare = letter("");
    let nodes = [
        logo.layout_node(),
        captioned.layout_node(),
        ornament.layout_node(),
        bare.layout_node(),
    ];
    let button = StyledContainer::new(
        LayoutStyle::new().width(30.0).height(30.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .control(Role::Button)
    .on_press(|| {})
    .a11y_label(|| "Close");
    let button_node = button.layout_node();
    let root = Container::new(LayoutStyle::new().flex_column(), vec![box_item(button)]).unwrap();
    compute_layout(
        root.layout_node(),
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();

    let square = rect(0.0, 0.0, 10.0, 10.0);
    let mut frame = Vec::new();
    for node in nodes {
        frame.push(open(node, square));
        frame.push(image_at(square));
        if node == nodes[1] {
            frame.push(text_at("Caption", square));
        }
        frame.push(DrawCommand::PopElement);
    }
    frame.push(image_at(square));
    frame.push(open(button_node, rect(0.0, 0.0, 30.0, 30.0)));
    frame.push(image_at(square));
    frame.push(DrawCommand::PopElement);
    let reading = FrameReading::of(&frame);

    let art = reading.artwork();
    assert_eq!(art.len(), 6);
    assert!(
        art.iter()
            .all(|art| matches!(frame[art.index], DrawCommand::Image { .. }))
    );
    let [logo, captioned, ornament, bare, outside, in_button] = art else {
        unreachable!()
    };
    assert_eq!(logo.node, Some(nodes[0]));
    assert_eq!(logo.name.as_deref(), Some("Company logo"));
    assert!(!logo.skipped);
    assert_eq!(
        captioned.name, None,
        "the card's name is its words', not the picture's"
    );
    assert!(ornament.skipped);
    assert_eq!(ornament.name, None);
    assert!(!bare.skipped && bare.name.is_none() && !bare.in_control);
    assert_eq!(outside.node, None);
    assert!(!outside.skipped && !outside.in_control);
    assert!(in_button.in_control);
    assert_eq!(in_button.node, Some(button_node));
}
