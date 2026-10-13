use std::sync::Arc;

use geometry_core::Rect;
use renderer_core::{Color, DrawCommand, Element, ElementId, Semantics, TextStyle};

use super::*;

struct Fixed;

impl Surface for Fixed {
    fn hold_document_scroll(&mut self, _id: u64) {}

    fn fixed_origin(&mut self) -> Option<(f32, f32)> {
        None
    }

    fn host_origin(&mut self) -> (f32, f32) {
        (0.0, 0.0)
    }

    fn image_href(&mut self, _data: &ImageData) -> Option<Rc<str>> {
        None
    }

    fn baseline(&mut self, style: &TextStyle) -> f32 {
        style.font_size
    }
}

fn open(id: u64, semantics: Semantics) -> DrawCommand {
    DrawCommand::PushElement {
        element: Arc::new(Element::new(
            ElementId(id),
            semantics,
            "",
            Rect::new(0.0, 0.0, 50.0, 20.0),
        )),
    }
}

fn text(content: &str) -> DrawCommand {
    DrawCommand::Text {
        text: Arc::from(content),
        spans: None,
        rect: Rect::new(0.0, 0.0, 50.0, 20.0),
        style: Arc::new(TextStyle::new(14.0, Color::BLACK)),
    }
}

fn only_box(frame: &Frame) -> &BoxNode {
    match frame.children.as_slice() {
        [Node::Box(node)] => node,
        _ => panic!("one box at the top"),
    }
}

#[test]
fn a_single_run_of_text_is_the_elements_own_text() {
    let frame = describe_frame(
        &[
            open(1, Semantics::group()),
            text("Hello"),
            DrawCommand::PopElement,
        ],
        None,
        &mut Fixed,
        false,
    );
    let node = only_box(&frame);
    assert!(matches!(&node.content, Content::Text { text, runs: None } if text == "Hello"));
    assert!(node.style.contains("font-size:14px;"), "{}", node.style);
}

#[test]
fn a_line_drawn_inside_its_box_is_set_in_from_the_edges_it_was_drawn_away_from() {
    let line = DrawCommand::Text {
        text: Arc::from("Workshop"),
        spans: None,
        rect: Rect::new(0.0, 11.0, 50.0, 17.0),
        style: Arc::new(TextStyle::new(14.0, Color::BLACK)),
    };
    let stretched = DrawCommand::PushElement {
        element: Arc::new(Element::new(
            ElementId(1),
            Semantics::group(),
            "padding:4px;",
            Rect::new(12.0, 0.0, 50.0, 40.0),
        )),
    };
    let frame = describe_frame(
        &[stretched, line, DrawCommand::PopElement],
        None,
        &mut Fixed,
        false,
    );
    let node = only_box(&frame);
    assert!(matches!(&node.content, Content::Text { text, .. } if text == "Workshop"));
    assert!(
        node.style.ends_with("padding:11px 0px 12px 0px;"),
        "the line's own rect is the content box, over whatever padding layout asked for: {}",
        node.style
    );

    let frame = describe_frame(
        &[
            open(1, Semantics::group()),
            text("Hello"),
            DrawCommand::PopElement,
        ],
        None,
        &mut Fixed,
        false,
    );
    assert!(
        !only_box(&frame).style.contains("padding"),
        "a line that fills its box needs no padding: {}",
        only_box(&frame).style
    );
}

#[test]
fn two_runs_of_text_are_pieces_placed_inside_their_box() {
    let frame = describe_frame(
        &[
            open(1, Semantics::group()),
            text("one"),
            text("two"),
            DrawCommand::PopElement,
        ],
        None,
        &mut Fixed,
        false,
    );
    let node = only_box(&frame);
    let Content::Children { children, pieces } = &node.content else {
        panic!("the box holds pieces");
    };
    assert!(children.is_empty());
    assert_eq!(pieces.len(), 2);
    assert!(pieces[0].style.starts_with("position:absolute;"));
    assert!(
        node.style.contains("position:absolute;"),
        "a layout root is positioned already: {}",
        node.style
    );
    assert!(!node.style.contains("position:relative;"));
}

#[test]
fn boxes_keep_the_order_they_were_drawn_in() {
    let frame = describe_frame(
        &[
            open(1, Semantics::group()),
            open(2, Semantics::group()),
            DrawCommand::PopElement,
            open(3, Semantics::group()),
            DrawCommand::PopElement,
            DrawCommand::PopElement,
        ],
        None,
        &mut Fixed,
        false,
    );
    let ids: Vec<u64> = children(only_box(&frame))
        .iter()
        .map(|node| node.key.id)
        .collect();
    assert_eq!(ids, [2, 3]);
}

#[test]
fn a_box_inside_a_drawing_is_part_of_its_picture() {
    let frame = describe_frame(
        &[
            open(1, Semantics::drawing().with_label("chart")),
            open(2, Semantics::group()),
            DrawCommand::PopElement,
            DrawCommand::PopElement,
        ],
        None,
        &mut Fixed,
        false,
    );
    let node = only_box(&frame);
    assert_eq!(node.tag, "svg");
    assert_eq!(node.described.role, Some("img"));
    assert!(matches!(&node.content, Content::Drawing(markup) if markup.contains("<g transform")));
}

#[test]
fn every_attribute_is_named_in_the_order_it_is_written() {
    let described = Described {
        role: Some("group"),
        label: Some("Card".to_string()),
        ..Described::default()
    };
    let names: Vec<&str> = described
        .attributes(BoxKey::from(5))
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert_eq!(
        names,
        [
            "role",
            "aria-label",
            "href",
            "target",
            "rel",
            "lang",
            "id",
            "aria-hidden",
            "aria-checked",
            "aria-pressed",
            "aria-selected",
            "aria-expanded",
            "aria-haspopup",
            "aria-current",
            "aria-keyshortcuts",
            "aria-controls",
            "aria-activedescendant",
            "aria-valuenow",
            "aria-valuemin",
            "aria-valuemax",
            "aria-orientation",
            "aria-level",
            "aria-posinset",
            "aria-setsize",
            "aria-disabled",
            "tabindex",
            CONSUMED_KEYS_ATTRIBUTE,
            FOCUS_BOX_ATTRIBUTE,
        ]
    );
}

#[test]
fn a_picture_is_named_by_its_alt_even_when_nobody_named_it() {
    let described = Described {
        picture: true,
        ..Described::default()
    };
    let attributes = described.attributes(BoxKey::from(5));
    assert!(attributes.contains(&("alt", Some(String::new()))));
    assert!(!attributes.iter().any(|(name, _)| *name == "aria-label"));
}

/// The attributes a box with `toggled` set ends up with, by role, leaving out the ones that are absent.
fn state_attributes(role: Role, on: bool) -> (&'static str, Vec<(&'static str, String)>) {
    let frame = describe_frame(
        &[
            open(1, Semantics::of(role).in_state(false, Some(on), false)),
            DrawCommand::PopElement,
        ],
        None,
        &mut Fixed,
        false,
    );
    let node = only_box(&frame);
    let states = node
        .described
        .attributes(node.key)
        .into_iter()
        .filter(|(name, _)| {
            matches!(
                *name,
                "aria-checked" | "aria-pressed" | "aria-selected" | "aria-expanded"
            )
        })
        .filter_map(|(name, value)| Some((name, value?)))
        .collect();
    (node.tag, states)
}

/// One flag, written as the attribute ARIA has for the role: a switch is checked, a toggle button pressed, a tab selected, a disclosure expanded.
#[test]
fn a_state_is_the_attribute_its_role_carries_it_in() {
    let on = |name: &'static str| vec![(name, "true".to_string())];
    assert_eq!(
        state_attributes(Role::Switch, true),
        ("div", on("aria-checked"))
    );
    assert_eq!(
        state_attributes(Role::Button, false),
        ("button", vec![("aria-pressed", "false".to_string())])
    );
    assert_eq!(
        state_attributes(Role::Tab, true),
        ("div", on("aria-selected"))
    );
    assert_eq!(
        state_attributes(Role::Disclosure, true),
        ("div", on("aria-expanded"))
    );
    assert_eq!(
        state_attributes(Role::MenuItemCheckBox, true),
        ("div", on("aria-checked"))
    );
    assert_eq!(
        state_attributes(Role::MenuItemRadio, false),
        ("div", vec![("aria-checked", "false".to_string())])
    );
    assert_eq!(
        state_attributes(Role::Slider, true),
        ("div", vec![]),
        "a role with no on/off state carries none"
    );
}

/// A tree row writes its selection, whether it is open and where it sits; a leaf writes no `aria-expanded` at all, which is how a reader knows there is nothing to open.
#[test]
fn a_tree_row_writes_its_selection_its_openness_and_its_place() {
    let attributes_of = |semantics: Semantics| {
        let frame = describe_frame(
            &[open(1, semantics), DrawCommand::PopElement],
            None,
            &mut Fixed,
            false,
        );
        let node = only_box(&frame);
        node.described
            .attributes(node.key)
            .into_iter()
            .filter(|(name, _)| name.starts_with("aria-") || *name == "role")
            .filter_map(|(name, value)| Some((name, value?)))
            .collect::<Vec<_>>()
    };
    let branch = Semantics {
        expanded: Some(false),
        position: Some(platform_core::SetPosition {
            level: 1,
            position: 2,
            size: 4,
        }),
        ..Semantics::of(Role::TreeItem).in_state(false, Some(true), false)
    };
    assert_eq!(
        attributes_of(branch),
        [
            ("role", "treeitem".to_string()),
            ("aria-selected", "true".to_string()),
            ("aria-expanded", "false".to_string()),
            ("aria-level", "1".to_string()),
            ("aria-posinset", "2".to_string()),
            ("aria-setsize", "4".to_string()),
        ]
    );
    let leaf = Semantics::of(Role::TreeItem).in_state(false, Some(false), false);
    assert_eq!(
        attributes_of(leaf),
        [
            ("role", "treeitem".to_string()),
            ("aria-selected", "false".to_string()),
        ]
    );
}

/// The `aria-current` a link marked current ends up with, `None` when it carries none.
fn current_attribute(semantics: Semantics) -> Option<String> {
    let frame = describe_frame(
        &[open(1, semantics), DrawCommand::PopElement],
        None,
        &mut Fixed,
        false,
    );
    let node = only_box(&frame);
    node.described
        .attributes(node.key)
        .into_iter()
        .find(|(name, _)| *name == "aria-current")
        .and_then(|(_, value)| value)
}

/// A route is the current page, an anchor the current location on it, and a language or an outside address just the current one; a link not marked, or one that goes nowhere, says nothing.
#[test]
fn a_current_link_says_what_it_is_the_current_one_of() {
    let link = |destination: Destination| Semantics::group().linking_to(destination);
    let route = Destination::Route(platform_core::Location::root().segment("about"));
    assert_eq!(
        current_attribute(link(route).marked_current(true)).as_deref(),
        Some("page")
    );
    assert_eq!(
        current_attribute(link(Destination::anchor("web")).marked_current(true)).as_deref(),
        Some("location")
    );
    assert_eq!(
        current_attribute(link(Destination::locale("en")).marked_current(true)).as_deref(),
        Some("true")
    );
    assert_eq!(current_attribute(link(Destination::anchor("web"))), None);
    assert_eq!(
        current_attribute(
            link(Destination::anchor("web"))
                .marked_current(true)
                .in_state(false, None, true)
        ),
        None,
        "a disabled link is no `<a href>`, so it is the current one of nothing"
    );
}

fn layer(id: u64, place: u64) -> DrawCommand {
    DrawCommand::PushElement {
        element: Arc::new(
            Element::new(
                ElementId(id),
                Semantics::group(),
                "display:flex;",
                Rect::new(0.0, 0.0, 800.0, 600.0),
            )
            .fixed_in_place_of(ElementId(place)),
        ),
    }
}

/// A page holding a layer's place (2) before a sticky stage (3), with the layer (10) and its bar (11) composed after the page, as a raster target draws them.
fn page_with_layer() -> Vec<DrawCommand> {
    vec![
        open(1, Semantics::group()),
        open(2, Semantics::group()),
        DrawCommand::PopElement,
        open(3, Semantics::group()),
        DrawCommand::PopElement,
        DrawCommand::PopElement,
        layer(10, 2),
        open(11, Semantics::group()),
        DrawCommand::PopElement,
        DrawCommand::PopElement,
    ]
}

/// Tab walks a document in the order of its elements, and so does a reader, so the layer goes where it was declared rather than after the page it is drawn over.
#[test]
fn a_layer_takes_the_place_it_was_declared_in() {
    let frame = describe_frame(&page_with_layer(), None, &mut Fixed, false);
    let page = only_box(&frame);
    let ids: Vec<u64> = children(page).iter().map(|node| node.key.id).collect();
    assert_eq!(ids, [10, 3], "the layer stands in for its place");
}

#[test]
fn a_layer_is_fixed_over_the_page_and_takes_the_pointer_only_over_its_boxes() {
    let frame = describe_frame(&page_with_layer(), None, &mut Fixed, false);
    let page = only_box(&frame);
    let layer = children(page)[0];
    for declaration in [
        "position:fixed;",
        "left:0px;",
        "top:0px;",
        "width:800px;",
        "height:600px;",
        "pointer-events:none;",
        "z-index:1;",
    ] {
        assert!(
            layer.style.contains(declaration),
            "{declaration} in {}",
            layer.style
        );
    }
    let bar = children(layer);
    assert!(
        bar[0].style.contains("pointer-events:auto;"),
        "{}",
        bar[0].style
    );
    assert!(
        page.style.contains("isolation:isolate;"),
        "the lift stays inside the root it is declared in, under what is placed after it: {}",
        page.style
    );
}

#[test]
fn a_layer_whose_place_is_missing_still_stands_over_the_surface() {
    let frame = describe_frame(
        &[
            open(1, Semantics::group()),
            DrawCommand::PopElement,
            layer(10, 99),
            DrawCommand::PopElement,
        ],
        None,
        &mut Fixed,
        false,
    );
    let ids: Vec<u64> = frame
        .children
        .iter()
        .map(|node| match node {
            Node::Box(node) => node.key.id,
            Node::Paint(_) => 0,
        })
        .collect();
    assert_eq!(ids, [1, 10]);
}

fn open_at(id: u64, semantics: Semantics, rect: Rect) -> DrawCommand {
    DrawCommand::PushElement {
        element: Arc::new(Element::new(ElementId(id), semantics, "", rect)),
    }
}

fn filled(rect: Rect) -> DrawCommand {
    DrawCommand::Rect {
        rect,
        style: Arc::new(renderer_core::RectStyle::filled(Color::WHITE, 0.0)),
    }
}

fn drawn(commands: &[DrawCommand]) -> String {
    let frame = describe_frame(commands, None, &mut Fixed, false);
    match &only_box(&frame).content {
        Content::Drawing(markup) => markup.clone(),
        _ => panic!("the box is a drawing"),
    }
}

/// A box scaled 1.5x from its start, 128px down a drawing that is itself 300px down the page: the scale is about (56, 468) on the surface, which is (0, 40) in the box.
#[test]
fn a_box_inside_a_drawing_transforms_and_cuts_itself_where_it_stands() {
    let at = Rect::new(56.0, 428.0, 200.0, 80.0);
    let markup = drawn(&[
        open_at(1, Semantics::drawing(), Rect::new(0.0, 300.0, 400.0, 200.0)),
        open_at(2, Semantics::group(), at),
        DrawCommand::PushClip {
            rect: at,
            radius: renderer_core::BorderRadius::zero(),
        },
        DrawCommand::PushMatrix {
            matrix: [1.5, 0.0, 0.0, 1.5, -28.0, -234.0],
        },
        filled(at),
        DrawCommand::PopMatrix,
        DrawCommand::PopClip,
        DrawCommand::PopElement,
        DrawCommand::PopElement,
    ]);
    assert!(
        markup.contains("<g transform=\"translate(56,128)\">"),
        "{markup}"
    );
    assert!(
        markup.contains("<rect x=\"0\" y=\"0\" width=\"200\" height=\"80\"/></clipPath>"),
        "{markup}"
    );
    assert!(
        markup.contains("<g transform=\"matrix(1.5,0,0,1.5,0,-20)\">"),
        "{markup}"
    );
    assert!(
        markup.contains("<rect x=\"0\" y=\"0\" width=\"200\" height=\"80\" fill=\"#ffffff\"/>"),
        "{markup}"
    );
}

/// What a canvas draws starts at its own corner, transforms included, wherever the canvas is on the page.
#[test]
fn artwork_inside_a_drawing_keeps_its_own_coordinates() {
    let markup = drawn(&[
        open_at(
            1,
            Semantics::drawing(),
            Rect::new(40.0, 300.0, 100.0, 100.0),
        ),
        DrawCommand::PushMatrix {
            matrix: [2.0, 0.0, 0.0, 2.0, 0.0, 0.0],
        },
        filled(Rect::new(10.0, 10.0, 20.0, 20.0)),
        DrawCommand::PopMatrix,
        DrawCommand::PushMatrix {
            matrix: [2.0, 0.0, 0.0, 2.0, 0.0, 0.0],
        },
        filled(Rect::new(0.0, 0.0, 100.0, 100.0)),
        DrawCommand::PopMatrix,
        DrawCommand::PopElement,
    ]);
    assert_eq!(
        markup,
        "<g transform=\"matrix(2,0,0,2,0,0)\"><rect x=\"10\" y=\"10\" width=\"20\" height=\"20\" fill=\"#ffffff\"/></g>\
<g transform=\"matrix(2,0,0,2,0,0)\"><rect x=\"0\" y=\"0\" width=\"100\" height=\"100\" fill=\"#ffffff\"/></g>"
    );
}

/// A frame drawn at the box's own corner says its matrix is in the box's coordinates too, so there is nothing to rebase.
#[test]
fn a_matrix_around_a_frame_at_the_corner_is_already_the_boxs_own() {
    let frame = describe_frame(
        &[
            open_at(1, Semantics::group(), Rect::new(200.0, 100.0, 100.0, 50.0)),
            DrawCommand::PushMatrix {
                matrix: [1.5, 0.0, 0.0, 1.5, 0.0, -12.5],
            },
            filled(Rect::new(0.0, 0.0, 100.0, 50.0)),
            DrawCommand::PopMatrix,
            DrawCommand::PopElement,
        ],
        None,
        &mut Fixed,
        false,
    );
    let node = only_box(&frame);
    assert!(
        node.style
            .contains("transform:matrix(1.5,0,0,1.5,0,-12.5);"),
        "{}",
        node.style
    );
}

/// `status` and `log` are live regions by their own definition, so the role alone makes a reader announce what changes in them.
#[test]
fn the_workbench_roles_are_the_aria_roles_of_the_same_name() {
    for (role, aria) in [
        (Role::TabList, "tablist"),
        (Role::Tree, "tree"),
        (Role::TreeItem, "treeitem"),
        (Role::Toolbar, "toolbar"),
        (Role::Splitter, "separator"),
        (Role::Status, "status"),
        (Role::Log, "log"),
        (Role::Menu, "menu"),
        (Role::MenuItem, "menuitem"),
        (Role::MenuItemCheckBox, "menuitemcheckbox"),
        (Role::MenuItemRadio, "menuitemradio"),
        (Role::ListBox, "listbox"),
        (Role::Option, "option"),
    ] {
        assert_eq!(aria_role(role), Some(aria), "{role:?}");
        assert_eq!(tag_of(&role), "div", "{role:?}");
    }
}

#[test]
fn a_splitter_writes_its_value_and_its_orientation() {
    let semantics = Semantics {
        value: Some(platform_core::NumericValue {
            now: 240.0,
            min: 180.0,
            max: 400.0,
        }),
        orientation: Some(platform_core::Orientation::Vertical),
        ..Semantics::of(Role::Splitter)
    };
    let frame = describe_frame(
        &[open(1, semantics), DrawCommand::PopElement],
        None,
        &mut Fixed,
        false,
    );
    let node = only_box(&frame);
    let written: Vec<_> = node
        .described
        .attributes(node.key)
        .into_iter()
        .filter(|(name, _)| name.starts_with("aria-value") || *name == "aria-orientation")
        .filter_map(|(name, value)| Some((name, value?)))
        .collect();
    assert_eq!(
        written,
        vec![
            ("aria-valuenow", "240".to_string()),
            ("aria-valuemin", "180".to_string()),
            ("aria-valuemax", "400".to_string()),
            ("aria-orientation", "vertical".to_string()),
        ]
    );
}

/// A menu button and a select's combobox write the kind of popup they open, beside whether it is open; a control that opens nothing writes no `aria-haspopup` at all.
#[test]
fn a_control_writes_what_it_opens() {
    let popup_of = |semantics: Semantics| {
        let frame = describe_frame(
            &[open(1, semantics), DrawCommand::PopElement],
            None,
            &mut Fixed,
            false,
        );
        let node = only_box(&frame);
        (
            attribute(node, "aria-haspopup"),
            attribute(node, "aria-expanded"),
        )
    };
    let opening = |role: Role, popup: HasPopup, open: bool| Semantics {
        has_popup: Some(popup),
        expanded: Some(open),
        ..Semantics::of(role)
    };
    assert_eq!(
        popup_of(opening(Role::Button, HasPopup::Menu, false)),
        (Some("menu".to_string()), Some("false".to_string()))
    );
    assert_eq!(
        popup_of(opening(Role::ComboBox, HasPopup::ListBox, true)),
        (Some("listbox".to_string()), Some("true".to_string()))
    );
    assert_eq!(popup_of(Semantics::of(Role::Button)), (None, None));
}

fn attribute(node: &BoxNode, name: &str) -> Option<String> {
    node.described
        .attributes(node.key)
        .into_iter()
        .find(|(written, _)| *written == name)
        .and_then(|(_, value)| value)
}

fn children(node: &BoxNode) -> Vec<&BoxNode> {
    match &node.content {
        Content::Children { children, .. } => children.iter().filter_map(Node::as_box).collect(),
        _ => panic!("a box with boxes inside"),
    }
}

/// A tree keeps focus while its cursor walks the rows: it points at the row the cursor is on by that row's `id`, which is named after the box when no anchor names it, and only that row is given one.
#[test]
fn a_container_points_at_the_row_its_cursor_rests_on() {
    let tree = |cursor: u64, anchored: Option<&str>| {
        let mut row = Semantics::of(Role::TreeItem);
        row.anchor = anchored.map(Into::into);
        describe_frame(
            &[
                open(
                    1,
                    Semantics {
                        active_descendant: Some(cursor),
                        ..Semantics::of(Role::Tree)
                    },
                ),
                open(2, Semantics::of(Role::TreeItem)),
                DrawCommand::PopElement,
                open(3, Semantics::group()),
                open(4, row),
                DrawCommand::PopElement,
                DrawCommand::PopElement,
                DrawCommand::PopElement,
            ],
            None,
            &mut Fixed,
            false,
        )
    };

    let frame = tree(4, None);
    let root = only_box(&frame);
    let first = children(root)[0];
    let nested = children(children(root)[1])[0];
    assert_eq!(
        attribute(root, "aria-activedescendant").as_deref(),
        Some("telar-box-4")
    );
    assert_eq!(attribute(nested, "id").as_deref(), Some("telar-box-4"));
    assert_eq!(
        attribute(first, "id"),
        None,
        "a row the cursor is not on has no id"
    );

    let frame = tree(4, Some("docs"));
    let root = only_box(&frame);
    assert_eq!(
        attribute(root, "aria-activedescendant").as_deref(),
        Some("docs"),
        "a row an anchor names is pointed at by that name"
    );

    let frame = tree(9, None);
    assert_eq!(
        attribute(only_box(&frame), "aria-activedescendant"),
        None,
        "a cursor on a row the frame did not draw points at nothing"
    );
}

fn named(semantics: Semantics, label: &str) -> Semantics {
    Semantics {
        label: Some(label.into()),
        ..semantics
    }
}

/// A box with no role that only wraps a control is that control's name, the way `<label>` is around its field; the group would otherwise carry a name no field has.
#[test]
fn a_named_wrapper_names_the_one_control_it_wraps() {
    let frame = describe_frame(
        &[
            open(1, named(Semantics::group(), "Speed")),
            open(2, Semantics::of(Role::SpinButton)),
            DrawCommand::PopElement,
            DrawCommand::PopElement,
        ],
        None,
        &mut Fixed,
        false,
    );
    let wrapper = only_box(&frame);
    let control = children(wrapper)[0];
    assert_eq!(attribute(wrapper, "aria-label"), None);
    assert_eq!(attribute(wrapper, "role"), None);
    assert_eq!(attribute(control, "aria-label").as_deref(), Some("Speed"));
}

/// A combobox, a spin button or a slider nothing named is named by the text drawn inside it, as every other target names it, where a browser would take that text for its value and leave the control nameless. A button is named by its content by the browser itself, and a wrapper's name still reaches the control it wraps.
#[test]
fn a_control_a_browser_would_not_name_by_its_content_is_named_by_what_it_draws() {
    let frame = describe_frame(
        &[
            open(1, Semantics::group()),
            open(2, Semantics::of(Role::ComboBox)),
            open(3, Semantics::group()),
            text("Small"),
            DrawCommand::PopElement,
            DrawCommand::PopElement,
            open(4, Semantics::of(Role::SpinButton)),
            open(5, Semantics::group()),
            text("Width"),
            DrawCommand::PopElement,
            open(6, Semantics::group()),
            text(" 13 "),
            DrawCommand::PopElement,
            DrawCommand::PopElement,
            open(7, Semantics::of(Role::Button)),
            text("Save"),
            DrawCommand::PopElement,
            open(8, named(Semantics::group(), "Speed")),
            open(9, Semantics::of(Role::Slider)),
            text("40"),
            DrawCommand::PopElement,
            DrawCommand::PopElement,
            DrawCommand::PopElement,
        ],
        None,
        &mut Fixed,
        false,
    );
    let [combobox, spin_button, button, wrapper] = children(only_box(&frame))[..] else {
        panic!("four boxes inside the root");
    };
    assert_eq!(attribute(combobox, "aria-label").as_deref(), Some("Small"));
    assert_eq!(
        attribute(spin_button, "aria-label").as_deref(),
        Some("Width 13")
    );
    assert_eq!(attribute(button, "aria-label"), None);
    assert_eq!(
        attribute(children(wrapper)[0], "aria-label").as_deref(),
        Some("Speed 40")
    );
}

/// A control hidden with `display:none` is still an element, and it is not one a reader would be sent to: only the visible control is counted and named.
#[test]
fn a_hidden_control_does_not_stop_a_wrapper_naming_the_visible_one() {
    let hidden = Element::new(
        ElementId(3),
        Semantics::of(Role::TextInput),
        "display:none;",
        Rect::new(0.0, 0.0, 50.0, 20.0),
    );
    let frame = describe_frame(
        &[
            open(1, named(Semantics::group(), "Speed")),
            open(2, Semantics::of(Role::SpinButton)),
            DrawCommand::PopElement,
            DrawCommand::PushElement {
                element: Arc::new(hidden),
            },
            DrawCommand::PopElement,
            DrawCommand::PopElement,
        ],
        None,
        &mut Fixed,
        false,
    );
    let wrapper = only_box(&frame);
    assert_eq!(attribute(wrapper, "aria-label"), None);
    assert_eq!(
        attribute(children(wrapper)[0], "aria-label").as_deref(),
        Some("Speed")
    );
    assert_eq!(attribute(children(wrapper)[1], "aria-label"), None);
}

/// A tab list has a role of its own, and a box with a caption beside its button has words of its own: each names itself, and its control keeps its own name.
#[test]
fn a_named_box_with_a_role_or_content_of_its_own_names_itself() {
    let list = describe_frame(
        &[
            open(1, named(Semantics::of(Role::TabList), "Sections")),
            open(2, Semantics::of(Role::Tab)),
            DrawCommand::PopElement,
            DrawCommand::PopElement,
        ],
        None,
        &mut Fixed,
        false,
    );
    let list = only_box(&list);
    assert_eq!(attribute(list, "aria-label").as_deref(), Some("Sections"));
    assert_eq!(attribute(children(list)[0], "aria-label"), None);

    let bar = describe_frame(
        &[
            open(1, named(Semantics::group(), "Fixed bar")),
            text("Fixed layer"),
            open(2, Semantics::of(Role::Button)),
            DrawCommand::PopElement,
            DrawCommand::PopElement,
        ],
        None,
        &mut Fixed,
        false,
    );
    let bar = only_box(&bar);
    assert_eq!(attribute(bar, "aria-label").as_deref(), Some("Fixed bar"));
    assert_eq!(attribute(bar, "role").as_deref(), Some("group"));
    assert_eq!(attribute(children(bar)[0], "aria-label"), None);
}

/// A bound that is not a number is no bound: `-inf` is not a value `aria-valuemin` can take, and a field with no limit has none to state.
#[test]
fn an_unbounded_value_writes_no_bounds() {
    let semantics = Semantics {
        value: Some(platform_core::NumericValue {
            now: 3.0,
            min: f64::NEG_INFINITY,
            max: f64::INFINITY,
        }),
        ..Semantics::of(Role::SpinButton)
    };
    let frame = describe_frame(
        &[open(1, semantics), DrawCommand::PopElement],
        None,
        &mut Fixed,
        false,
    );
    let node = only_box(&frame);
    assert_eq!(attribute(node, "aria-valuenow").as_deref(), Some("3"));
    assert_eq!(attribute(node, "aria-valuemin"), None);
    assert_eq!(attribute(node, "aria-valuemax"), None);
}

/// A host away from the viewport's corner, so a box placed against the viewport and one placed inside a box are told apart.
struct Offset;

impl Surface for Offset {
    fn hold_document_scroll(&mut self, _id: u64) {}

    fn fixed_origin(&mut self) -> Option<(f32, f32)> {
        None
    }

    fn host_origin(&mut self) -> (f32, f32) {
        (100.0, 200.0)
    }

    fn image_href(&mut self, _data: &ImageData) -> Option<Rc<str>> {
        None
    }

    fn baseline(&mut self, style: &TextStyle) -> f32 {
        style.font_size
    }
}

const NESTED: u64 = 7;

/// `content`, the frame of surface [`NESTED`], drawn in the window's box 1.
fn framed(content: Vec<DrawCommand>) -> Vec<DrawCommand> {
    let mut commands = vec![
        open_at(1, Semantics::group(), Rect::new(30.0, 40.0, 200.0, 100.0)),
        DrawCommand::PushSurface { surface: NESTED },
    ];
    commands.extend(content);
    commands.extend([DrawCommand::PopSurface, DrawCommand::PopElement]);
    commands
}

fn in_nested(id: u64) -> BoxKey {
    BoxKey::new(Some(NESTED), id)
}

/// Each surface numbers its boxes from its own counter, so the window's box and a box of the surface drawn in it can share an id and still be two elements.
#[test]
fn a_nested_surfaces_boxes_are_named_by_the_surface_that_numbered_them() {
    let focusable = Semantics::of(Role::Button).focusable(renderer_core::Focusable {
        tab_stop: true,
        consumes: Role::Button.consumed_keys(),
    });
    let frame = describe_frame(
        &framed(vec![
            open_at(1, Semantics::group(), Rect::new(0.0, 0.0, 200.0, 100.0)),
            open_at(2, focusable, Rect::new(10.0, 10.0, 50.0, 20.0)),
            DrawCommand::PopElement,
            DrawCommand::PopElement,
        ]),
        None,
        &mut Fixed,
        false,
    );
    let window_box = only_box(&frame);
    assert_eq!(window_box.key, BoxKey::from(1));
    let root = children(window_box)[0];
    assert_eq!(root.key, in_nested(1));
    let button = children(root)[0];
    assert_eq!(button.key, in_nested(2));
    assert_eq!(
        attribute(button, platform_core::consumed_keys::FOCUS_BOX_ATTRIBUTE).as_deref(),
        Some("7-2"),
        "a focus the document moves onto it names the surface too"
    );
}

/// The surface computed and placed its own roots, as the window did its own: nothing in the box it is drawn in says where they go, so each is placed at its rect inside that box, which contains them.
#[test]
fn a_nested_surfaces_layout_roots_are_placed_in_the_box_it_is_drawn_in() {
    let frame = describe_frame(
        &framed(vec![
            open_at(1, Semantics::group(), Rect::new(0.0, 0.0, 200.0, 100.0)),
            DrawCommand::PopElement,
            open_at(2, Semantics::group(), Rect::new(20.0, 30.0, 60.0, 40.0)),
            DrawCommand::PopElement,
        ]),
        None,
        &mut Offset,
        false,
    );
    let window_box = only_box(&frame);
    assert!(
        window_box.style.contains("contain:layout;"),
        "{}",
        window_box.style
    );
    let roots = children(window_box);
    let keys: Vec<BoxKey> = roots.iter().map(|root| root.key).collect();
    assert_eq!(keys, [in_nested(1), in_nested(2)]);
    for declaration in [
        "position:absolute;",
        "left:20px;",
        "top:30px;",
        "width:60px;",
        "height:40px;",
    ] {
        assert!(
            roots[1].style.contains(declaration),
            "{declaration} in {}",
            roots[1].style
        );
    }
}

#[test]
fn a_nested_surfaces_layer_is_fixed_over_that_surface_and_lifted_inside_its_root() {
    let frame = describe_frame(&framed(page_with_layer()), None, &mut Offset, false);
    let window_box = only_box(&frame);
    let page = children(window_box)[0];
    assert_eq!(page.key, in_nested(1));
    let layer = children(page)[0];
    assert_eq!(
        layer.key,
        in_nested(10),
        "the layer stands in for its place"
    );
    for declaration in ["position:fixed;", "left:0px;", "top:0px;", "z-index:1;"] {
        assert!(
            layer.style.contains(declaration),
            "{declaration} in {}",
            layer.style
        );
    }
    assert!(
        page.style.contains("isolation:isolate;"),
        "the lift stays inside the surface's own root: {}",
        page.style
    );
    assert!(
        !window_box.style.contains("isolation:"),
        "and not the window's: {}",
        window_box.style
    );
}

#[test]
fn a_layer_of_the_window_takes_no_place_a_nested_surface_numbered() {
    let mut commands = framed(vec![
        open_at(2, Semantics::group(), Rect::new(0.0, 0.0, 200.0, 100.0)),
        DrawCommand::PopElement,
    ]);
    commands.extend([layer(10, 2), DrawCommand::PopElement]);
    let frame = describe_frame(&commands, None, &mut Fixed, false);
    let keys: Vec<BoxKey> = frame
        .children
        .iter()
        .filter_map(|node| match node {
            Node::Box(node) => Some(node.key),
            Node::Paint(_) => None,
        })
        .collect();
    assert_eq!(keys, [BoxKey::from(1), BoxKey::from(10)]);
    assert_eq!(children(only_box_of(&frame, 1))[0].key, in_nested(2));
}

fn only_box_of(frame: &Frame, id: u64) -> &BoxNode {
    frame
        .children
        .iter()
        .find_map(|node| match node {
            Node::Box(node) if node.key == BoxKey::from(id) => Some(&**node),
            _ => None,
        })
        .expect("the box is a root")
}

#[test]
fn a_cursor_points_only_at_a_row_of_its_own_surface() {
    let tree = Semantics {
        active_descendant: Some(2),
        ..Semantics::of(Role::Tree)
    };
    let frame = describe_frame(
        &[
            open(1, tree),
            DrawCommand::PushSurface { surface: NESTED },
            open(2, Semantics::of(Role::TreeItem)),
            DrawCommand::PopElement,
            DrawCommand::PopSurface,
            DrawCommand::PopElement,
        ],
        None,
        &mut Fixed,
        false,
    );
    let root = only_box(&frame);
    assert_eq!(attribute(root, "aria-activedescendant"), None);
    assert_eq!(attribute(children(root)[0], "id"), None);
}

#[test]
fn a_nested_surface_never_takes_the_document_scroll() {
    let frame = describe_frame(
        &[
            DrawCommand::PushSurface { surface: NESTED },
            DrawCommand::PushElement {
                element: Arc::new(
                    Element::new(
                        ElementId(1),
                        Semantics::of(Role::ScrollArea),
                        "",
                        Rect::new(0.0, 0.0, 200.0, 100.0),
                    )
                    .as_primary_scroll(true),
                ),
            },
            DrawCommand::PopElement,
            DrawCommand::PopSurface,
        ],
        None,
        &mut Fixed,
        false,
    );
    assert_eq!(frame.primary, None);
    let root = only_box(&frame);
    assert_eq!(root.key, in_nested(1));
    assert!(root.scrolls, "it scrolls as a box of its own");
}

#[test]
fn a_drawing_in_a_nested_surface_names_its_definitions_apart_from_the_windows() {
    let drawing = |surface: Option<u64>| {
        let gradient = renderer_core::ShapeStyle::with_fill(
            renderer_core::RectStyle::default(),
            renderer_core::Paint::Gradient(renderer_core::Gradient::linear(
                geometry_core::Point::new(0.0, 0.0),
                geometry_core::Point::new(20.0, 0.0),
                &[(0.0, Color::BLACK), (1.0, Color::WHITE)],
            )),
        );
        let mut commands = vec![
            open_at(
                4,
                Semantics::of(Role::Drawing),
                Rect::new(0.0, 0.0, 20.0, 20.0),
            ),
            DrawCommand::Rect {
                rect: Rect::new(0.0, 0.0, 20.0, 20.0),
                style: Arc::new(gradient),
            },
            DrawCommand::PopElement,
        ];
        if let Some(surface) = surface {
            commands.insert(0, DrawCommand::PushSurface { surface });
            commands.push(DrawCommand::PopSurface);
        }
        match &only_box(&describe_frame(&commands, None, &mut Fixed, false)).content {
            Content::Drawing(markup) => markup.clone(),
            _ => panic!("a drawing"),
        }
    };
    assert!(drawing(None).contains("id=\"t4-1\""), "{}", drawing(None));
    assert!(
        drawing(Some(NESTED)).contains("id=\"t7-4-1\""),
        "{}",
        drawing(Some(NESTED))
    );
}

/// A modal inside a frame dims the page under it and not itself: the scrim the surface paints at its own level, between its page and its dialog, stays between them.
fn framed_page_scrim_dialog() -> Vec<DrawCommand> {
    framed(vec![
        open_at(1, Semantics::group(), Rect::new(0.0, 0.0, 200.0, 100.0)),
        DrawCommand::PopElement,
        filled(Rect::new(0.0, 0.0, 200.0, 100.0)),
        open_at(2, Semantics::group(), Rect::new(50.0, 20.0, 100.0, 60.0)),
        DrawCommand::PopElement,
    ])
}

#[test]
fn paint_a_nested_surface_carries_at_its_own_level_keeps_its_place_among_its_roots() {
    let frame = describe_frame(&framed_page_scrim_dialog(), None, &mut Offset, false);
    let window_box = only_box(&frame);
    let Content::Children { children, pieces } = &window_box.content else {
        panic!("the box the surface is drawn in holds its top level");
    };
    assert!(pieces.is_empty(), "the scrim is no piece after the roots");
    let order: Vec<Option<BoxKey>> = children
        .iter()
        .map(|child| child.as_box().map(|node| node.key))
        .collect();
    assert_eq!(order, [Some(in_nested(1)), None, Some(in_nested(2))]);
    let Node::Paint(scrim) = &children[1] else {
        panic!("the scrim is paint");
    };
    for declaration in [
        "position:absolute;",
        "left:0px;",
        "top:0px;",
        "width:200px;",
        "height:100px;",
        "pointer-events:none;",
    ] {
        assert!(
            scrim.style.contains(declaration),
            "{declaration} in {}",
            scrim.style
        );
    }
    assert!(
        !window_box.style.contains("background"),
        "a surface's own paint the size of the box is not the box's background: {}",
        window_box.style
    );
}

#[test]
fn what_the_box_paints_before_the_surface_is_still_its_own() {
    let commands = [
        open_at(1, Semantics::group(), Rect::new(30.0, 40.0, 200.0, 100.0)),
        filled(Rect::new(30.0, 40.0, 200.0, 100.0)),
        DrawCommand::PushSurface { surface: NESTED },
        open_at(1, Semantics::group(), Rect::new(0.0, 0.0, 200.0, 100.0)),
        DrawCommand::PopElement,
        DrawCommand::PopSurface,
        DrawCommand::PopElement,
    ];
    let frame = describe_frame(&commands, None, &mut Fixed, false);
    let window_box = only_box(&frame);
    assert!(
        window_box.style.contains("background"),
        "{}",
        window_box.style
    );
    assert_eq!(children(window_box)[0].key, in_nested(1));
}

fn roots(frame: &Frame) -> Vec<&BoxNode> {
    frame.children.iter().filter_map(Node::as_box).collect()
}

/// An open combobox keeps focus on itself and names the list it opened, which is laid over the page far from it: the list gets an `id` to be named by, wherever it is in the frame, and the combobox's cursor is looked for inside that list.
#[test]
fn a_combobox_names_the_list_it_controls_and_points_into_it() {
    let combobox = Semantics {
        controls: Some(3),
        active_descendant: Some(5),
        ..Semantics::of(Role::ComboBox)
    };
    let frame = describe_frame(
        &[
            open(1, Semantics::group()),
            open(2, combobox),
            DrawCommand::PopElement,
            DrawCommand::PopElement,
            open(3, Semantics::of(Role::ListBox)),
            open(
                4,
                Semantics::of(Role::Option).in_state(false, Some(true), false),
            ),
            DrawCommand::PopElement,
            open(
                5,
                Semantics::of(Role::Option).in_state(false, Some(false), false),
            ),
            DrawCommand::PopElement,
            DrawCommand::PopElement,
        ],
        None,
        &mut Fixed,
        false,
    );
    let roots = roots(&frame);
    let combobox = children(roots[0])[0];
    let list = roots[1];
    assert_eq!(attribute(list, "role").as_deref(), Some("listbox"));
    let options = children(list);
    assert_eq!(attribute(options[0], "role").as_deref(), Some("option"));
    assert_eq!(
        attribute(options[0], "aria-selected").as_deref(),
        Some("true")
    );
    assert_eq!(
        attribute(options[1], "aria-selected").as_deref(),
        Some("false"),
        "an option says it is not the chosen one rather than nothing"
    );
    assert_eq!(
        attribute(combobox, "aria-controls").as_deref(),
        Some("telar-box-3")
    );
    assert_eq!(attribute(list, "id").as_deref(), Some("telar-box-3"));
    assert_eq!(
        attribute(combobox, "aria-activedescendant").as_deref(),
        Some("telar-box-5")
    );
    let rows = children(list);
    assert_eq!(attribute(rows[1], "id").as_deref(), Some("telar-box-5"));
    assert_eq!(attribute(rows[0], "id"), None);
}

/// The list is named the same way when it comes first in the frame, and named by its anchor when it has one.
#[test]
fn a_controlled_box_before_its_control_or_with_an_anchor_is_named_too() {
    let mut list = Semantics::of(Role::Menu);
    list.anchor = Some("choices".into());
    let frame = describe_frame(
        &[
            open(3, list),
            DrawCommand::PopElement,
            open(
                2,
                Semantics {
                    controls: Some(3),
                    ..Semantics::of(Role::Button)
                },
            ),
            DrawCommand::PopElement,
        ],
        None,
        &mut Fixed,
        false,
    );
    let roots = roots(&frame);
    assert_eq!(
        attribute(roots[1], "aria-controls").as_deref(),
        Some("choices")
    );
}

/// A box that controls one the frame did not draw names nothing, and a cursor it names outside both itself and what it controls points at nothing.
#[test]
fn a_control_naming_a_box_the_frame_did_not_draw_names_nothing() {
    let frame = describe_frame(
        &[
            open(
                2,
                Semantics {
                    controls: Some(9),
                    active_descendant: Some(4),
                    ..Semantics::of(Role::ComboBox)
                },
            ),
            DrawCommand::PopElement,
            open(4, Semantics::of(Role::MenuItem)),
            DrawCommand::PopElement,
        ],
        None,
        &mut Fixed,
        false,
    );
    let roots = roots(&frame);
    assert_eq!(attribute(roots[0], "aria-controls"), None);
    assert_eq!(attribute(roots[0], "aria-activedescendant"), None);
    assert_eq!(attribute(roots[1], "id"), None);
}

/// A box of a nested surface names only a box of its own surface, which numbers its boxes from its own counter.
#[test]
fn a_control_names_only_a_box_of_its_own_surface() {
    let frame = describe_frame(
        &[
            open(
                1,
                Semantics {
                    controls: Some(2),
                    ..Semantics::of(Role::ComboBox)
                },
            ),
            DrawCommand::PushSurface { surface: NESTED },
            open(2, Semantics::of(Role::Menu)),
            DrawCommand::PopElement,
            DrawCommand::PopSurface,
            DrawCommand::PopElement,
        ],
        None,
        &mut Fixed,
        false,
    );
    let root = only_box(&frame);
    assert_eq!(attribute(root, "aria-controls"), None);
    assert_eq!(attribute(children(root)[0], "id"), None);
}

/// A control that runs the same command as a key writes it as `aria-keyshortcuts`; one with no such key writes none.
#[test]
fn a_control_writes_the_keys_that_run_its_command() {
    let keys_of = |semantics: Semantics| {
        let frame = describe_frame(
            &[open(1, semantics), DrawCommand::PopElement],
            None,
            &mut Fixed,
            false,
        );
        attribute(only_box(&frame), "aria-keyshortcuts")
    };
    let bound = Semantics {
        keyboard_shortcut: Some("Control+Shift+Z".into()),
        ..Semantics::of(Role::MenuItem)
    };
    assert_eq!(keys_of(bound), Some("Control+Shift+Z".to_string()));
    assert_eq!(keys_of(Semantics::of(Role::MenuItem)), None);
}
