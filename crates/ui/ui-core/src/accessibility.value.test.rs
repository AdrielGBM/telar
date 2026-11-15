use std::cell::Cell;
use std::rc::Rc;

use layout_core::{AvailableSpace, LayoutStyle};
use platform_core::{NumericValue, TextSelection, Value};
use reactive_core::{RwSignal, signal};
use renderer_core::{Color, RectStyle, TextStyle};
use ui_tree::ComponentList;

use super::*;
use crate::annotation::Accessible;
use crate::container::Container;
use crate::context::{compute_layout, reset_layout_runtime};
use crate::input::Input;
use crate::layout_item::{LayoutItem, box_item};
use crate::styled_container::StyledContainer;
use crate::text::Text;

fn unrouted(event: &Event) {
    panic!("an action on a control of this surface travels no tree: {event:?}")
}

fn field(value: RwSignal<String>) -> Input {
    Input::new(value, LayoutStyle::new().width(160.0).height(20.0), || {
        TextStyle::new(12.0, Color::BLACK)
    })
    .unwrap()
}

fn mounted(items: Vec<Box<dyn LayoutItem>>) -> ComponentList {
    let root = Container::new(LayoutStyle::new().flex_column(), items).unwrap();
    compute_layout(
        root.layout_node(),
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();
    ComponentList::new(root)
}

fn read(tree: &ComponentList) -> Vec<AccessNode> {
    snapshot(&tree.commands())
}

fn the(nodes: &[AccessNode], role: Role) -> AccessNode {
    nodes
        .iter()
        .find(|node| node.role == role)
        .cloned()
        .unwrap_or_else(|| panic!("a {role:?} is read: {nodes:#?}"))
}

fn text_value(node: &AccessNode) -> Option<&str> {
    node.value.as_ref().and_then(Value::text)
}

/// A field says what it holds as its value, under the name it was given; the drawn text never becomes part of that name.
#[test]
fn a_labelled_text_field_carries_its_text_as_its_value() {
    reset_layout_runtime();
    focus::clear();
    let tree = mounted(vec![box_item(
        field(signal("Ada Lovelace".to_string())).a11y_label(|| "Name"),
    )]);

    let node = the(&read(&tree), Role::TextInput);
    assert_eq!(node.name, "Name");
    assert_eq!(text_value(&node), Some("Ada Lovelace"));
    assert!(node.supports.set_value && node.supports.set_selection);
}

/// A field nobody named stays unnamed, as a document leaves an `<input>` with no label: what is typed into it, or the hint it shows while empty, is what it holds and never what it is called.
#[test]
fn an_unnamed_field_is_not_named_by_its_text_or_its_hint() {
    reset_layout_runtime();
    focus::clear();
    let tree = mounted(vec![
        box_item(field(signal("draft".to_string()))),
        box_item(field(signal(String::new())).placeholder("Search")),
    ]);

    let fields: Vec<AccessNode> = read(&tree)
        .into_iter()
        .filter(|node| node.role == Role::TextInput)
        .collect();
    assert_eq!(fields.len(), 2);
    assert!(
        fields.iter().all(|node| node.name.is_empty()),
        "{fields:#?}"
    );
    assert_eq!(text_value(&fields[0]), Some("draft"));
    assert_eq!(text_value(&fields[1]), Some(""));
}

/// A secret field is read as the mask it shows, never as what was typed.
#[test]
fn a_secret_field_is_read_as_its_mask() {
    reset_layout_runtime();
    focus::clear();
    let tree = mounted(vec![box_item(
        field(signal("hunter2".to_string()))
            .masked('•')
            .a11y_label(|| "Password"),
    )]);

    assert_eq!(
        text_value(&the(&read(&tree), Role::TextInput)),
        Some("•••••••")
    );
}

/// The caret of the field that holds the keyboard is read with it, counted in characters, so a reader follows it through the text.
#[test]
fn the_focused_field_says_where_its_caret_is() {
    reset_layout_runtime();
    focus::clear();
    let input = field(signal("añil".to_string())).a11y_label(|| "Colour");
    let id = input.focus_id();
    let tree = mounted(vec![box_item(input)]);
    focus::request(id);

    assert_eq!(
        the(&read(&tree), Role::TextInput).selection,
        Some(TextSelection {
            anchor: 4,
            focus: 4
        })
    );
}

/// A reader writing a whole value into a field replaces what it holds, and one placing the caret or a selection puts it there, each counted in characters.
#[test]
fn a_reader_sets_a_fields_text_and_its_selection() {
    reset_layout_runtime();
    focus::clear();
    let value = signal("old".to_string());
    let input = field(value).a11y_label(|| "Title");
    let id = input.focus_id();
    let tree = mounted(vec![box_item(input)]);

    perform_action(
        AccessId::own(id),
        AccessAction::SetValue(Value::Text("añil\nazul".into())),
        unrouted,
    );
    assert_eq!(value.get(), "añil", "a field of one line keeps the first");

    focus::request(id);
    perform_action(
        AccessId::own(id),
        AccessAction::SetSelection(TextSelection {
            anchor: 3,
            focus: 1,
        }),
        unrouted,
    );
    assert_eq!(
        the(&read(&tree), Role::TextInput).selection,
        Some(TextSelection {
            anchor: 3,
            focus: 1
        })
    );
}

/// A selection the platform moved by itself in the element a document types into reaches the field that holds the keyboard, the same way a reader's does.
#[test]
fn a_selection_the_platform_moved_reaches_the_focused_field() {
    reset_layout_runtime();
    focus::clear();
    let input = field(signal("hello".to_string())).a11y_label(|| "Greeting");
    let id = input.focus_id();
    let mut tree = mounted(vec![box_item(input)]);
    focus::request(id);

    tree.on_event(&Event::SelectionMoved {
        selection: TextSelection {
            anchor: 0,
            focus: 2,
        },
    });
    assert_eq!(
        the(&read(&tree), Role::TextInput).selection,
        Some(TextSelection {
            anchor: 0,
            focus: 2
        })
    );
}

/// A select's combobox says the choice it shows as its value, under its own name; one given no name is called by its choice too, as a document calls it.
#[test]
fn a_select_carries_its_chosen_label_as_its_value() {
    reset_layout_runtime();
    focus::clear();
    let combobox = |name: Option<&'static str>| {
        let chosen = Text::new(
            || "Medium".to_string(),
            LayoutStyle::new().width(80.0).height(20.0),
            || TextStyle::new(12.0, Color::BLACK),
        )
        .unwrap();
        let control = StyledContainer::new(
            LayoutStyle::new().padding_all(4.0),
            |_r| RectStyle::default(),
            vec![box_item(chosen)],
        )
        .unwrap()
        .control(Role::ComboBox);
        match name {
            Some(name) => box_item(control.a11y_label(move || name)),
            None => box_item(control),
        }
    };
    let tree = mounted(vec![combobox(Some("Size")), combobox(None)]);

    let selects: Vec<AccessNode> = read(&tree)
        .into_iter()
        .filter(|node| node.role == Role::ComboBox)
        .collect();
    assert_eq!(selects.len(), 2);
    assert_eq!(selects[0].name, "Size");
    assert_eq!(selects[1].name, "Medium");
    assert!(
        selects
            .iter()
            .all(|node| text_value(node) == Some("Medium"))
    );
}

/// A reader stepping a slider moves it as its arrow keys do, one step at a time, and says so only where the slider registered how.
#[test]
fn increment_on_a_slider_raises_its_value_by_one_step() {
    reset_layout_runtime();
    focus::clear();
    let level = signal(0.5_f64);
    let stepped = Rc::new(Cell::new(0));
    let slider = StyledContainer::new(
        LayoutStyle::new().width(200.0).height(20.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .control(Role::Slider)
    .a11y_label(|| "Volume")
    .valued(move || NumericValue::new(0.0..=1.0, level.get()).ok())
    .stepped({
        let stepped = stepped.clone();
        move |step| {
            stepped.set(stepped.get() + 1);
            let by = match step {
                focus::Step::Increment => 0.1,
                focus::Step::Decrement => -0.1,
            };
            level.set((level.peek() + by).clamp(0.0, 1.0));
            true
        }
    });
    let id = slider.focus_id().expect("a control has an id");
    let tree = mounted(vec![box_item(slider)]);
    let now = |tree: &ComponentList| {
        the(&read(tree), Role::Slider)
            .value
            .as_ref()
            .and_then(Value::reading)
            .expect("a slider carries a number")
            .now()
    };
    assert!(the(&read(&tree), Role::Slider).supports.step);

    perform_action(AccessId::own(id), AccessAction::Increment, unrouted);
    assert!((now(&tree) - 0.6).abs() < 1e-9);
    perform_action(AccessId::own(id), AccessAction::Decrement, unrouted);
    assert!((now(&tree) - 0.5).abs() < 1e-9);
    assert_eq!(stepped.get(), 2);

    perform_action(
        AccessId::own(id),
        AccessAction::SetValue(Value::Text("loud".into())),
        unrouted,
    );
    assert!(
        (now(&tree) - 0.5).abs() < 1e-9,
        "a slider that takes no value is left where it was"
    );
}

fn bar(filled: RwSignal<f64>) -> StyledContainer {
    StyledContainer::new(
        LayoutStyle::new().width(100.0).height(8.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .role(Role::ProgressBar)
    .valued(move || NumericValue::new(0.0..=100.0, filled.get()).ok())
}

/// A box the keyboard never stops at still says what it shows, named or not, on a target that builds a document and on one that does not: how far a bar has filled, re-read as it fills.
#[test]
fn a_box_that_is_no_control_carries_its_value() {
    for capturing in [false, true] {
        let was = ui_tree::set_element_capture(capturing);
        reset_layout_runtime();
        focus::clear();
        let filled = signal(40.0);
        let tree = mounted(vec![
            box_item(bar(filled).a11y_label(|| "Upload")),
            box_item(bar(filled)),
        ]);
        let bars = |tree: &ComponentList| -> Vec<(String, Option<f64>, bool)> {
            read(tree)
                .into_iter()
                .filter(|node| node.role == Role::ProgressBar)
                .map(|node| {
                    let now = node
                        .value
                        .as_ref()
                        .and_then(Value::reading)
                        .map(|r| r.now());
                    (node.name, now, node.id.is_some())
                })
                .collect()
        };
        let before = bars(&tree);
        filled.set(70.0);
        let after = bars(&tree);
        ui_tree::set_element_capture(was);

        assert_eq!(
            before,
            [
                ("Upload".to_string(), Some(40.0), false),
                (String::new(), Some(40.0), false),
            ],
            "capturing elements: {capturing}"
        );
        assert_eq!(
            after.iter().map(|(_, now, _)| *now).collect::<Vec<_>>(),
            [Some(70.0), Some(70.0)],
            "capturing elements: {capturing}"
        );
    }
}
