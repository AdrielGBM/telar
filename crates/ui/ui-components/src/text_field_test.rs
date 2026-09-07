use std::rc::Rc;

use layout_core::AvailableSpace;
use platform_core::{Event, Key, ModifiersState, NamedKey};
use renderer_core::DrawCommand;

use super::*;
use crate::harness::press;
use ui_core::{ComponentList, compute_layout, new_container, track_layout};

// Returns the field and its absolute box rect. Valid only when `label` is empty: a labelled field's outer node is the wrapping column, offset above the box by the caption row.
fn laid_out_field(props: TextFieldProps) -> (Box<dyn LayoutItem>, geometry_core::Rect) {
    crate::test_support::fresh_layout_runtime();
    let field = text_field(props, Children::default()).unwrap();
    let root = new_container(
        LayoutStyle::new().flex_column().width(400.0).height(200.0),
        &[field.layout_node()],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();
    let rect = track_layout(field.layout_node()).unwrap().get();
    (field, rect)
}

fn find_text(cmds: &[DrawCommand], needle: &str) -> bool {
    cmds.iter()
        .any(|c| matches!(c, DrawCommand::Text { text, .. } if text.as_ref() == needle))
}

/// The §1.4 bug, made unwritable: a properties panel that says its region is 11px used to get a field at 14.98 among 11px labels, with no attribute to correct it — `TextFieldProps` has no size.
#[test]
fn a_field_takes_the_size_the_region_around_it_declared() {
    let value = signal("hi".to_string());
    crate::test_support::fresh_layout_runtime();
    let field = text_field(
        TextFieldProps::props().value(value).build(),
        Children::default(),
    )
    .unwrap();
    let box_node = field.layout_node();
    let root = new_container(
        LayoutStyle::new().flex_column().width(400.0).height(200.0),
        &[box_node],
    )
    .unwrap();
    ui_core::declare(
        root,
        renderer_core::Declared::default().with_font_size(11.0),
    );
    compute_layout(
        root,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();

    let tree = ComponentList::new(field);
    let size = tree
        .commands()
        .iter()
        .find_map(|c| match c {
            DrawCommand::Text { text, style, .. } if text.as_ref() == "hi" => Some(style.font_size),
            _ => None,
        })
        .expect("the field drew its value");
    assert_eq!(size, 11.0);

    // The box is sized before the field has a parent to inherit from, so this half arrives by effect. Within a pixel because taffy rounds, and 4px clear of the 39.6 an unfollowed box would have kept.
    let height = track_layout(box_node).unwrap().get().height;
    assert!(
        (height - (2.0 * pad_y() + ui_core::single_line_box(11.0))).abs() < 1.0,
        "the box closes down to the text instead of keeping the room the theme's size wanted, got {height}"
    );
}

#[test]
fn controlled_value_renders_as_input_text() {
    let value = signal("hi".to_string());
    let (field, _) = laid_out_field(TextFieldProps::props().value(value).build());
    let tree = ComponentList::new(field);
    assert!(
        find_text(&tree.commands(), "hi"),
        "a controlled value is what the field draws"
    );
}

#[test]
fn empty_value_renders_placeholder() {
    let (field, _) = laid_out_field(TextFieldProps::props().placeholder("Search…").build());
    let tree = ComponentList::new(field);
    assert!(
        find_text(&tree.commands(), "Search…"),
        "an empty field draws its placeholder instead"
    );
}

#[test]
fn typing_into_a_focused_field_edits_the_bound_signal() {
    let value = signal("a".to_string());
    let (field, rect) = laid_out_field(TextFieldProps::props().value(value).build());
    let mut tree = ComponentList::new(field);
    let _ = tree.commands(); // build the initial segments before dispatching events

    let press_x = (rect.x + pad_x() + 2.0) as f64;
    let press_y = (rect.y + pad_y() + 2.0) as f64;
    tree.on_event(&press(press_x, press_y));
    tree.on_event(&Event::KeyPressed {
        key: Key::Char('z'),
        modifiers: ModifiersState::default(),
    });

    assert_eq!(value.get(), "az");
}

#[test]
fn enter_fires_on_submit_while_focused() {
    use std::cell::Cell;

    let value = signal("a".to_string());
    let fired = Rc::new(Cell::new(false));
    let sink = fired.clone();
    let (field, rect) = laid_out_field(
        TextFieldProps::props()
            .value(value)
            .on_submit(Rc::new(move || sink.set(true)) as Rc<dyn Fn()>)
            .build(),
    );
    let mut tree = ComponentList::new(field);
    let _ = tree.commands();

    let press_x = (rect.x + pad_x() + 2.0) as f64;
    let press_y = (rect.y + pad_y() + 2.0) as f64;
    tree.on_event(&press(press_x, press_y));
    tree.on_event(&Event::KeyPressed {
        key: Key::Named(NamedKey::Enter),
        modifiers: ModifiersState::default(),
    });

    assert!(fired.get(), "Enter while focused should fire on_submit");
}

#[test]
fn label_adds_a_caption_row_above_the_box() {
    let (field, _) = laid_out_field(
        TextFieldProps::props()
            .label("Name")
            .placeholder("Type your name")
            .build(),
    );
    let tree = ComponentList::new(field);
    assert!(
        find_text(&tree.commands(), "Name"),
        "the caption row draws the label"
    );
    assert!(
        find_text(&tree.commands(), "Type your name"),
        "and the box still draws its placeholder"
    );
}
