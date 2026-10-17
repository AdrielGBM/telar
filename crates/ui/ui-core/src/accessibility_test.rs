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
        read.ends_with("\ntreeitem, focused") && !read.contains("tree, focused"),
        "a reader says the row is focused, not the tree: {read:?}"
    );

    cursor.set(elsewhere);
    assert_eq!(tree_node(&snapshot(&[])).active_descendant, None);
    focus::clear();
}

/// An open combobox keeps focus and names the list it opened, which is somewhere else on the screen; a list that is gone is named by nobody.
#[test]
fn a_combobox_names_the_list_it_controls() {
    reset_layout_runtime();
    focus::clear();
    let list = StyledContainer::new(
        LayoutStyle::new().width(100.0).height(40.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .presented(Role::ListBox);
    let list_id = list.focus_id().unwrap();
    let gone = focus::next_id();
    let target = reactive_core::signal(list_id);
    let combobox = StyledContainer::new(
        LayoutStyle::new().width(100.0).height(20.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .control(Role::ComboBox)
    .controls(move || Some(target.get()));
    let combobox_id = combobox.focus_id().unwrap();
    let root = Container::new(
        LayoutStyle::new().flex_column(),
        vec![box_item(combobox), box_item(list)],
    )
    .unwrap();
    compute_layout(
        root.layout_node(),
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();
    focus::request(combobox_id);

    let combobox = |nodes: &[AccessNode]| {
        nodes
            .iter()
            .find(|n| n.role == Role::ComboBox)
            .cloned()
            .expect("the combobox is exposed")
    };
    assert_eq!(combobox(&snapshot(&[])).controls, Some(list_id));
    target.set(gone);
    assert_eq!(combobox(&snapshot(&[])).controls, None);
    focus::clear();
}

/// A scroll area places its content in a space of its own and moves it by the scroll offset, so a control inside one is reported where it is drawn: in window space, the same space its label is read in.
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

/// A menu opened over a page covers the page's text: the heading under its first item is the page's, not part of what the item is called.
#[test]
fn text_a_control_is_drawn_over_is_not_part_of_its_name() {
    reset_layout_runtime();
    focus::clear();
    let item = StyledContainer::new(
        LayoutStyle::new().width(120.0).height(30.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .control(Role::MenuItem)
    .on_press(|| {});
    let node = item.layout_node();
    let root = Container::new(LayoutStyle::new().flex_column(), vec![box_item(item)]).unwrap();
    compute_layout(
        root.layout_node(),
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();

    let nodes = snapshot(&[
        text_at("Page title", rect(10.0, 5.0, 60.0, 16.0)),
        open(node, rect(0.0, 0.0, 120.0, 30.0)),
        text_at("Default", rect(10.0, 7.0, 50.0, 16.0)),
        DrawCommand::PopElement,
        text_at("Hint", rect(80.0, 7.0, 30.0, 16.0)),
    ]);
    let item = nodes
        .iter()
        .find(|node| node.id.is_some())
        .expect("the item is exposed");
    assert_eq!(
        item.name, "Default Hint",
        "what it draws, and what is drawn over it"
    );
    assert!(
        nodes
            .iter()
            .any(|node| node.id.is_none() && node.name == "Page title"),
        "the covered heading is still the page's: {nodes:?}"
    );
}

/// A wide named box an open overlay covers stays its own node: the overlay control is not named by it.
#[test]
fn a_named_box_under_an_overlay_control_is_not_part_of_its_name() {
    use crate::annotation::Accessible;
    reset_layout_runtime();
    focus::clear();
    let overlay = StyledContainer::new(
        LayoutStyle::new().absolute_fill(),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .control(Role::MenuItem)
    .on_press(|| {});
    let node = overlay.layout_node();
    let status = StyledContainer::new(
        LayoutStyle::new().width(200.0).height(20.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .role(Role::Status)
    .a11y_label(|| "Header status");
    let status_node = status.layout_node();
    let root = Container::new(
        LayoutStyle::new().flex_column(),
        vec![box_item(status), box_item(overlay)],
    )
    .unwrap();
    compute_layout(
        root.layout_node(),
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();

    let nodes = snapshot(&[
        open(status_node, rect(0.0, 0.0, 200.0, 20.0)),
        DrawCommand::PopElement,
        open(node, rect(0.0, 0.0, 400.0, 200.0)),
        text_at("Copy", rect(10.0, 40.0, 40.0, 16.0)),
        DrawCommand::PopElement,
    ]);
    let item = nodes
        .iter()
        .find(|node| node.id.is_some())
        .expect("the overlay control is exposed");
    assert_eq!(item.name, "Copy");
    assert!(
        read_on_its_own(&nodes, "Header status"),
        "the covered box keeps its own node: {nodes:?}"
    );
}

/// A pane folded away clips its content to nothing: what it drew is not on screen to be read.
#[test]
fn text_clipped_to_nothing_is_not_announced() {
    reset_layout_runtime();
    focus::clear();
    let clip = |rect| DrawCommand::PushClip {
        rect,
        radius: renderer_core::BorderRadius::zero(),
    };
    let nodes = snapshot(&[
        clip(rect(0.0, 40.0, 0.0, 300.0)),
        clip(rect(0.0, 40.0, 200.0, 20.0)),
        text_at("Folded", rect(0.0, 40.0, 60.0, 16.0)),
        DrawCommand::PopClip,
        DrawCommand::PopClip,
        clip(rect(0.0, 40.0, 200.0, 20.0)),
        text_at("Shown", rect(10.0, 40.0, 40.0, 16.0)),
        DrawCommand::PopClip,
        text_at("Unclipped", rect(10.0, 80.0, 0.0, 0.0)),
    ]);
    let names: Vec<&str> = nodes.iter().map(|node| node.name.as_str()).collect();
    assert_eq!(names, ["Shown", "Unclipped"]);
}

#[test]
fn an_id_a_nested_surface_was_placed_under_names_its_control_there() {
    let node = AccessNode {
        id: Some(3),
        parent: None,
        role: Role::Button,
        name: "Inside".to_string(),
        rect: rect(0.0, 0.0, 10.0, 10.0),
        focused: false,
        enabled: true,
        toggled: None,
        value: None,
        orientation: None,
        expanded: None,
        position: None,
        lang: None,
        url: None,
        current: None,
        active_descendant: None,
        controls: None,
        has_popup: None,
        keyboard_shortcut: None,
        sort: None,
        multiselectable: false,
        modal: false,
    };
    let nested = |surface| NestedSurface {
        surface,
        commands: 0..0,
        placement: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
    };
    let placed = nested(7).place(node.clone()).id.expect("an id");
    assert_eq!(
        AccessTarget::of(placed),
        AccessTarget::Nested { surface: 7, id: 3 }
    );
    let deeper = nested(2).place(nested(7).place(node)).id.expect("an id");
    assert_eq!(
        AccessTarget::of(deeper),
        AccessTarget::Nested { surface: 7, id: 3 },
        "a surface nested deeper keeps its own handle in the id"
    );
    assert_eq!(AccessTarget::of(3), AccessTarget::Own(3));
}

/// While a modal layer is up a reader keeps to it, as Tab does: the page's text under it is not read, the layer's is, and a dialog in it is modal. The control it was opened from is still read by what it draws, and closing the layer gives the page back.
#[test]
fn a_modal_layer_that_is_up_keeps_a_reader_inside_it() {
    use crate::annotation::Accessible;
    use crate::{Overlay, WindowRoot, relayout_if_dirty};

    reset_layout_runtime();
    focus::clear();
    let words = |text: &'static str| {
        box_item(
            Text::new(
                move || text.to_string(),
                LayoutStyle::new().width(120.0).height(16.0),
                || TextStyle::new(12.0, renderer_core::Color::BLACK),
            )
            .unwrap(),
        )
    };
    let open = reactive_core::signal(true);
    let opener = StyledContainer::new(
        LayoutStyle::new().width(120.0).height(30.0),
        |_r| RectStyle::default(),
        vec![words("Choose")],
    )
    .unwrap()
    .control(Role::Button)
    .on_press(|| {});
    let opener_node = opener.layout_node();
    let dialog = StyledContainer::new(
        LayoutStyle::new().width(200.0).height(100.0),
        |_r| RectStyle::default(),
        vec![words("Accept the terms")],
    )
    .unwrap()
    .role(Role::Dialog)
    .a11y_label(|| "Terms");
    let layer = Overlay::toggleable(LayoutStyle::new(), vec![box_item(dialog)], move || {
        open.get()
    })
    .unwrap()
    .owned_by(opener_node);
    let page = Container::new(
        LayoutStyle::new().flex_column(),
        vec![words("Background notes"), box_item(opener), box_item(layer)],
    )
    .unwrap();
    crate::set_surface_size(geometry_core::Size::new(400.0, 300.0));
    let mut tree = ui_tree::ComponentList::new(WindowRoot::wrapping(Box::new(page)).unwrap());
    tree.on_event(&Event::WindowResized {
        width: 400,
        height: 300,
    });
    relayout_if_dirty();

    let nodes = snapshot(&tree.commands());
    assert!(!read_on_its_own(&nodes, "Background notes"), "{nodes:#?}");
    assert!(read_on_its_own(&nodes, "Accept the terms"));
    let dialog = nodes
        .iter()
        .find(|n| n.role == Role::Dialog)
        .expect("the dialog is read");
    assert!(dialog.modal);
    assert_eq!(names_of(&nodes, Role::Button), ["Choose"]);

    open.set(false);
    relayout_if_dirty();
    let nodes = snapshot(&tree.commands());
    assert!(read_on_its_own(&nodes, "Background notes"));
    assert!(nodes.iter().all(|n| n.role != Role::Dialog && !n.modal));
}

/// An item scrolled out of view is drawn where its text may land on another control; it is named by the text it draws, wherever that is.
#[test]
fn a_control_is_named_by_its_text_wherever_scrolling_put_it() {
    reset_layout_runtime();
    focus::clear();
    let clip = |rect| DrawCommand::PushClip {
        rect,
        radius: renderer_core::BorderRadius::zero(),
    };
    let control = |role, width, height| {
        StyledContainer::new(
            LayoutStyle::new().width(width).height(height),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .control(role)
        .on_press(|| {})
    };
    let item = control(Role::Option, 120.0, 30.0);
    let hidden_item = control(Role::Option, 120.0, 30.0);
    let trigger = control(Role::Button, 60.0, 20.0);
    let (item_node, hidden_node, trigger_node) = (
        item.layout_node(),
        hidden_item.layout_node(),
        trigger.layout_node(),
    );
    let root = Container::new(
        LayoutStyle::new().flex_column(),
        vec![box_item(item), box_item(hidden_item), box_item(trigger)],
    )
    .unwrap();
    compute_layout(
        root.layout_node(),
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();

    let trigger_box = crate::input_region::visible_rect(trigger_node).unwrap();
    let nodes = snapshot(&[
        open(item_node, rect(0.0, 0.0, 120.0, 30.0)),
        text_at(
            "Beside the trigger",
            rect(trigger_box.x + 5.0, trigger_box.y + 2.0, 40.0, 16.0),
        ),
        DrawCommand::PopElement,
        clip(rect(0.0, 0.0, 0.0, 0.0)),
        open(hidden_node, rect(0.0, 30.0, 120.0, 30.0)),
        text_at("Folded away", rect(0.0, 30.0, 60.0, 16.0)),
        DrawCommand::PopElement,
        DrawCommand::PopClip,
        open(trigger_node, trigger_box),
        text_at(
            "Open",
            rect(trigger_box.x + 5.0, trigger_box.y + 2.0, 30.0, 16.0),
        ),
        DrawCommand::PopElement,
    ]);
    assert_eq!(
        names_of(&nodes, Role::Option),
        ["Beside the trigger", "Folded away"]
    );
    assert_eq!(names_of(&nodes, Role::Button), ["Open"]);
}

fn placed(position: u32, size: u32) -> platform_core::SetPosition {
    platform_core::SetPosition {
        level: 1,
        position,
        size,
    }
}

fn header_cell(caption: &'static str) -> StyledContainer {
    StyledContainer::new(
        LayoutStyle::new(),
        |_r| RectStyle::default(),
        vec![box_item(letter(caption))],
    )
    .unwrap()
    .control(Role::ColumnHeader)
    .on_press(|| {})
}

fn header_row(cells: Vec<StyledContainer>) -> StyledContainer {
    StyledContainer::new(
        LayoutStyle::new().flex_row(),
        |_r| RectStyle::default(),
        cells.into_iter().map(box_item).collect(),
    )
    .unwrap()
    .role(Role::Row)
    .positioned_by(|| placed(1, 40))
}

/// A box that is no control and has no name, but says where it sits among items a reader cannot count, is read as its role and its place: a grid's header row, ahead of the headers it holds, which keep their own names.
#[test]
fn an_unnamed_box_that_says_where_it_sits_is_read_for_its_place() {
    for capturing in [false, true] {
        let was = ui_tree::set_element_capture(capturing);
        reset_layout_runtime();
        focus::clear();
        let row = header_row(vec![header_cell("N"), header_cell("S")]);
        let root = Container::new(LayoutStyle::new().flex_column(), vec![box_item(row)]).unwrap();

        let nodes = snapshot_of(root);
        ui_tree::set_element_capture(was);
        let read: Vec<(Role, &str, Option<u32>)> = nodes
            .iter()
            .map(|n| (n.role, n.name.as_str(), n.position.map(|p| p.position)))
            .collect();
        assert_eq!(
            read,
            [
                (Role::Row, "", Some(1)),
                (Role::ColumnHeader, "N", None),
                (Role::ColumnHeader, "S", None),
            ],
            "capturing elements: {capturing}"
        );
        assert_eq!(nodes[0].id, None, "nothing addresses the row");
        assert_eq!(nodes[0].position, Some(placed(1, 40)));
        assert_eq!(
            platform_core::accessibility::transcript(&nodes[..1]),
            "row, 1 of 40"
        );
    }
}

/// An unnamed box is a node only for a place its role says: one that says no place, or a place its role does not take, has nothing a reader would hear, so a list of plain wrappers does not fill the reading with empty items.
#[test]
fn an_unnamed_box_with_no_place_to_say_is_not_read() {
    reset_layout_runtime();
    focus::clear();
    let unplaced = Container::new(LayoutStyle::new(), vec![box_item(letter("A"))])
        .unwrap()
        .role(Role::ListItem);
    let unroled = StyledContainer::new(
        LayoutStyle::new(),
        |_r| RectStyle::default(),
        vec![box_item(letter("B"))],
    )
    .unwrap()
    .positioned(placed(2, 3));
    let root = Container::new(
        LayoutStyle::new().flex_column(),
        vec![box_item(unplaced), box_item(unroled)],
    )
    .unwrap();

    let nodes = snapshot_of(root);
    let read: Vec<(Role, &str)> = nodes.iter().map(|n| (n.role, n.name.as_str())).collect();
    assert_eq!(read, [(Role::Label, "A"), (Role::Label, "B")]);
}

/// A placed box a reader is told to skip is skipped with everything in it, as any other box is.
#[test]
fn a_hidden_placed_box_is_not_read() {
    use crate::annotation::Accessible;
    reset_layout_runtime();
    focus::clear();
    let row = header_row(vec![]).a11y_hidden();
    let root = Container::new(LayoutStyle::new().flex_column(), vec![box_item(row)]).unwrap();
    assert_eq!(snapshot_of(root), []);
}

/// A named box that says where it sits keeps its place with its name: a named list item is read as one item of so many.
#[test]
fn a_named_box_keeps_its_place() {
    use crate::annotation::Accessible;
    reset_layout_runtime();
    focus::clear();
    let item = StyledContainer::new(
        LayoutStyle::new().width(40.0).height(20.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .role(Role::ListItem)
    .positioned(placed(4, 9))
    .a11y_label(|| "Chapter four");
    let root = Container::new(LayoutStyle::new().flex_column(), vec![box_item(item)]).unwrap();

    let nodes = snapshot_of(root);
    assert_eq!(nodes.len(), 1, "{nodes:?}");
    assert_eq!(nodes[0].name, "Chapter four");
    assert_eq!(nodes[0].role, Role::ListItem);
    assert_eq!(nodes[0].position, Some(placed(4, 9)));
}

/// What the node at `parent` holds, or the window for `None`, by role and name in reading order.
fn held(nodes: &[AccessNode], parent: Option<usize>) -> Vec<(Role, &str)> {
    platform_core::accessibility::children_of(nodes, parent)
        .map(|child| (nodes[child].role, nodes[child].name.as_str()))
        .collect()
}

fn at(nodes: &[AccessNode], role: Role, name: &str) -> usize {
    nodes
        .iter()
        .position(|node| node.role == role && node.name == name)
        .unwrap_or_else(|| panic!("a {role:?} named {name:?} among {nodes:#?}"))
}

fn words(text: &'static str) -> Text {
    Text::new(
        move || text.to_string(),
        LayoutStyle::new().width(60.0).height(16.0),
        || TextStyle::new(12.0, renderer_core::Color::BLACK),
    )
    .unwrap()
}

fn presented(role: Role, children: Vec<Box<dyn LayoutItem>>) -> StyledContainer {
    StyledContainer::new(
        LayoutStyle::new().flex_row(),
        |_r| RectStyle::default(),
        children,
    )
    .unwrap()
    .presented(role)
}

/// Every node comes after the one it sits in, and the window's own come in reading order: what lets a flat reader walk the list and a tree reader walk the parents, and see the same thing.
fn assert_listed_as_a_tree(nodes: &[AccessNode]) {
    for (index, node) in nodes.iter().enumerate() {
        if let Some(parent) = node.parent {
            assert!(parent < index, "{index} sits in {parent}, listed after it");
            let between = &nodes[parent + 1..index];
            assert!(
                between.iter().all(|other| {
                    std::iter::successors(other.parent, |&above| nodes[above].parent)
                        .any(|above| above == parent)
                }),
                "everything between {parent} and {index} sits in {parent}"
            );
        }
    }
}

/// A reader steps into a grid by its rows and into a row by its cells: each cell is held by the row it is built in, and each row by the grid, whatever the boxes between them.
#[test]
fn a_grid_holds_its_rows_and_each_row_its_cells() {
    use crate::annotation::Accessible;
    reset_layout_runtime();
    focus::clear();
    let cell = |text| presented(Role::GridCell, vec![box_item(words(text))]);
    let header =
        header_row(vec![header_cell("N"), header_cell("S")]).positioned_by(|| placed(1, 3));
    let body = |first, second, place| {
        presented(
            Role::Row,
            vec![box_item(cell(first)), box_item(cell(second))],
        )
        .positioned(placed(place, 3))
    };
    let rows = Container::new(
        LayoutStyle::new().flex_column(),
        vec![box_item(body("a", "1", 2)), box_item(body("b", "2", 3))],
    )
    .unwrap();
    let grid = StyledContainer::new(
        LayoutStyle::new().flex_column(),
        |_r| RectStyle::default(),
        vec![box_item(header), box_item(rows)],
    )
    .unwrap()
    .control(Role::Grid)
    .a11y_label(|| "Files");
    let root = Container::new(
        LayoutStyle::new().flex_column(),
        vec![box_item(words("Above")), box_item(grid)],
    )
    .unwrap();

    let nodes = snapshot_of(root);
    assert_listed_as_a_tree(&nodes);
    assert_eq!(
        held(&nodes, None),
        [(Role::Label, "Above"), (Role::Grid, "Files")]
    );
    let grid = at(&nodes, Role::Grid, "Files");
    let rows: Vec<usize> = platform_core::accessibility::children_of(&nodes, Some(grid)).collect();
    let places: Vec<Option<u32>> = rows
        .iter()
        .map(|&row| nodes[row].position.map(|place| place.position))
        .collect();
    assert_eq!(places, [Some(1), Some(2), Some(3)]);
    assert!(rows.iter().all(|&row| nodes[row].role == Role::Row));
    assert_eq!(
        held(&nodes, Some(rows[0])),
        [(Role::ColumnHeader, "N"), (Role::ColumnHeader, "S")]
    );
    assert_eq!(
        held(&nodes, Some(rows[2])),
        [(Role::GridCell, "b"), (Role::GridCell, "2")]
    );
    assert_eq!(
        platform_core::accessibility::transcript(&nodes),
        "Above\nFiles, grid\n  row, 1 of 3\n    N, columnheader\n    S, columnheader\n  row, 2 of 3\n    a, gridcell\n    1, gridcell\n  row, 3 of 3\n    b, gridcell\n    2, gridcell",
        "the transcript indents each node under the one it sits in"
    );
}

/// A list box holds its options, and the label beside it stays beside it.
#[test]
fn a_list_box_holds_its_options() {
    reset_layout_runtime();
    focus::clear();
    let options = ["Apple", "Pear"]
        .into_iter()
        .enumerate()
        .map(|(i, fruit)| {
            box_item(
                presented(Role::Option, vec![box_item(words(fruit))]).positioned(SetPosition {
                    level: 1,
                    position: i as u32 + 1,
                    size: 2,
                }),
            )
        })
        .collect();
    let list = StyledContainer::new(
        LayoutStyle::new().flex_column(),
        |_r| RectStyle::default(),
        options,
    )
    .unwrap()
    .control(Role::ListBox);
    let root = Container::new(
        LayoutStyle::new().flex_column(),
        vec![box_item(words("Fruit")), box_item(list)],
    )
    .unwrap();

    let nodes = snapshot_of(root);
    assert_listed_as_a_tree(&nodes);
    assert_eq!(
        held(&nodes, None),
        [(Role::Label, "Fruit"), (Role::ListBox, "")]
    );
    let list = at(&nodes, Role::ListBox, "");
    assert_eq!(
        held(&nodes, Some(list)),
        [(Role::Option, "Apple"), (Role::Option, "Pear")]
    );
}

/// A tree whose rows are built inside the rows they hang from, each branch's children in a group of its own, reads the same way: a row holds the rows under it, and the group boxes between them, which say nothing, are not nodes.
#[test]
fn a_tree_row_holds_the_rows_nested_under_it() {
    reset_layout_runtime();
    focus::clear();
    let row = |name, children: Vec<Box<dyn LayoutItem>>| {
        let mut content: Vec<Box<dyn LayoutItem>> = vec![box_item(words(name))];
        if !children.is_empty() {
            let group = Container::new(LayoutStyle::new().flex_column(), children)
                .unwrap()
                .role(Role::Group);
            content.push(box_item(group));
        }
        StyledContainer::new(
            LayoutStyle::new().flex_column(),
            |_r| RectStyle::default(),
            content,
        )
        .unwrap()
        .presented(Role::TreeItem)
    };
    let primary = row("Primary", vec![]);
    let button = row("Button", vec![box_item(primary)]);
    let inputs = row("Inputs", vec![box_item(button)]);
    let about = row("About", vec![]);
    let tree = StyledContainer::new(
        LayoutStyle::new().flex_column(),
        |_r| RectStyle::default(),
        vec![box_item(inputs), box_item(about)],
    )
    .unwrap()
    .control(Role::Tree);
    let root = Container::new(LayoutStyle::new().flex_column(), vec![box_item(tree)]).unwrap();

    let nodes = snapshot_of(root);
    assert_listed_as_a_tree(&nodes);
    let tree = at(&nodes, Role::Tree, "");
    assert_eq!(
        held(&nodes, Some(tree)),
        [(Role::TreeItem, "Inputs"), (Role::TreeItem, "About")]
    );
    let inputs = at(&nodes, Role::TreeItem, "Inputs");
    assert_eq!(held(&nodes, Some(inputs)), [(Role::TreeItem, "Button")]);
    let button = at(&nodes, Role::TreeItem, "Button");
    assert_eq!(held(&nodes, Some(button)), [(Role::TreeItem, "Primary")]);
}

/// A dialog holds what it asks and the controls that answer it; text the application only drew there is the dialog's content, not a sibling of it.
#[test]
fn a_dialog_holds_its_content() {
    use crate::annotation::Accessible;
    reset_layout_runtime();
    focus::clear();
    let ok = StyledContainer::new(
        LayoutStyle::new().width(60.0).height(24.0),
        |_r| RectStyle::default(),
        vec![box_item(words("OK"))],
    )
    .unwrap()
    .control(Role::Button)
    .on_press(|| {});
    let dialog = StyledContainer::new(
        LayoutStyle::new().flex_column(),
        |_r| RectStyle::default(),
        vec![box_item(words("Delete it?")), box_item(ok)],
    )
    .unwrap()
    .role(Role::Dialog)
    .a11y_label(|| "Confirm");
    let root = Container::new(
        LayoutStyle::new().flex_column(),
        vec![box_item(dialog), box_item(words("Below"))],
    )
    .unwrap();

    let nodes = snapshot_of(root);
    assert_listed_as_a_tree(&nodes);
    assert_eq!(
        held(&nodes, None),
        [(Role::Dialog, "Confirm"), (Role::Label, "Below")]
    );
    let dialog = at(&nodes, Role::Dialog, "Confirm");
    assert_eq!(
        held(&nodes, Some(dialog)),
        [(Role::Label, "Delete it?"), (Role::Button, "OK")]
    );
}

/// A box with a structural role and no name is still the node a reader steps into, as long as it holds something: a tab list around its tabs. Holding nothing, it is not read at all.
#[test]
fn an_unnamed_tab_list_holds_its_tabs_and_an_empty_one_is_not_read() {
    reset_layout_runtime();
    focus::clear();
    let tabs = Container::new(
        LayoutStyle::new().flex_row(),
        vec![
            box_item(drawn_tab("General")),
            box_item(drawn_tab("Advanced")),
        ],
    )
    .unwrap()
    .role(Role::TabList);
    let empty = Container::new(LayoutStyle::new().width(40.0).height(20.0), vec![])
        .unwrap()
        .role(Role::TabList);
    let root = Container::new(
        LayoutStyle::new().flex_column(),
        vec![box_item(tabs), box_item(empty)],
    )
    .unwrap();

    let nodes = snapshot_of(root);
    assert_listed_as_a_tree(&nodes);
    assert_eq!(held(&nodes, None), [(Role::TabList, "")]);
    assert_eq!(
        held(&nodes, Some(0)),
        [(Role::Tab, "General"), (Role::Tab, "Advanced")]
    );
}

/// A box named as a group reads as its name, a caption beside what it groups, so what is inside it is read beside it rather than inside a label.
#[test]
fn a_named_group_is_read_beside_what_it_holds() {
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
    let root = Container::new(LayoutStyle::new().flex_column(), vec![box_item(pair)]).unwrap();

    let nodes = snapshot_of(root);
    assert_eq!(
        held(&nodes, None),
        [
            (Role::Label, "Sections"),
            (Role::Tab, "General"),
            (Role::Tab, "Advanced"),
        ]
    );
}

/// A menu opened in a modal layer holds its items, and the page under the layer is not read.
#[test]
fn a_menu_in_a_modal_layer_holds_its_items() {
    use crate::{Overlay, WindowRoot, relayout_if_dirty};

    reset_layout_runtime();
    focus::clear();
    let opener = StyledContainer::new(
        LayoutStyle::new().width(120.0).height(30.0),
        |_r| RectStyle::default(),
        vec![box_item(words("Edit"))],
    )
    .unwrap()
    .control(Role::Button)
    .on_press(|| {});
    let opener_node = opener.layout_node();
    let item = |name| {
        StyledContainer::new(
            LayoutStyle::new().width(120.0).height(24.0),
            |_r| RectStyle::default(),
            vec![box_item(words(name))],
        )
        .unwrap()
        .control(Role::MenuItem)
        .on_press(|| {})
    };
    let menu = StyledContainer::new(
        LayoutStyle::new().flex_column(),
        |_r| RectStyle::default(),
        vec![box_item(item("Cut")), box_item(item("Copy"))],
    )
    .unwrap()
    .control(Role::Menu);
    let layer = Overlay::toggleable(LayoutStyle::new(), vec![box_item(menu)], || true)
        .unwrap()
        .owned_by(opener_node);
    let page = Container::new(
        LayoutStyle::new().flex_column(),
        vec![box_item(words("Notes")), box_item(opener), box_item(layer)],
    )
    .unwrap();
    crate::set_surface_size(geometry_core::Size::new(400.0, 300.0));
    let mut tree = ui_tree::ComponentList::new(WindowRoot::wrapping(Box::new(page)).unwrap());
    tree.on_event(&Event::WindowResized {
        width: 400,
        height: 300,
    });
    relayout_if_dirty();

    let nodes = snapshot(&tree.commands());
    assert_listed_as_a_tree(&nodes);
    assert!(!read_on_its_own(&nodes, "Notes"), "{nodes:#?}");
    let menu = at(&nodes, Role::Menu, "");
    assert_eq!(
        held(&nodes, Some(menu)),
        [(Role::MenuItem, "Cut"), (Role::MenuItem, "Copy")]
    );
}

fn bare(role: Role, name: &str, at: Rect, parent: Option<usize>) -> AccessNode {
    AccessNode {
        parent,
        ..unaddressed(role, name.to_string(), at)
    }
}

/// A surface's snapshot joins the frame's under the node it is drawn in, its top nodes read among that node's own by where they fall, and every node keeps what it holds.
#[test]
fn a_grafted_snapshot_joins_the_node_it_is_drawn_in() {
    let frame = vec![
        bare(Role::Main, "Page", rect(0.0, 0.0, 400.0, 400.0), None),
        bare(Role::Label, "Top", rect(0.0, 0.0, 100.0, 20.0), Some(0)),
        bare(
            Role::Label,
            "Bottom",
            rect(0.0, 300.0, 100.0, 20.0),
            Some(0),
        ),
        bare(Role::Label, "Footer", rect(0.0, 500.0, 100.0, 20.0), None),
    ];
    let canvas = vec![
        bare(Role::List, "Inner", rect(0.0, 100.0, 100.0, 100.0), None),
        bare(Role::Label, "Item", rect(0.0, 100.0, 100.0, 20.0), Some(0)),
    ];
    let nodes = graft(frame, vec![(Some(0), canvas)]);
    assert_listed_as_a_tree(&nodes);
    assert_eq!(
        held(&nodes, Some(0)),
        [
            (Role::Label, "Top"),
            (Role::List, "Inner"),
            (Role::Label, "Bottom"),
        ]
    );
    let inner = at(&nodes, Role::List, "Inner");
    assert_eq!(held(&nodes, Some(inner)), [(Role::Label, "Item")]);
    assert_eq!(
        held(&nodes, None),
        [(Role::Main, "Page"), (Role::Label, "Footer")]
    );
}
