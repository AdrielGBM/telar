use std::rc::Rc;

use telar::testing::press;
use telar::{
    AvailableSpace, ComponentList, DrawCommand, Event, Key, ModifiersState, NamedKey,
    compute_layout, new_container, track_layout,
};

use super::*;

// Returns the field and its absolute box rect. Valid only when `label` is empty: a labelled field's outer node is the wrapping column, offset above the box by the caption row.
fn laid_out_field(props: TextFieldProps) -> (Box<dyn LayoutItem>, telar::Rect) {
    crate::test_support::fresh_layout_runtime();
    lay_out_field(props)
}

fn lay_out_field(props: TextFieldProps) -> (Box<dyn LayoutItem>, telar::Rect) {
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
    telar::declare(root, telar::Declared::default().with_font_size(11.0));
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

    let height = track_layout(box_node).unwrap().get().height;
    assert_eq!(
        height,
        shared::control_height(),
        "the box stays one control tall whatever size its text is, so it stands level with a button beside it"
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

    let press_x = (rect.x + TextFieldTokens::current().pad_x() + 2.0) as f64;
    let press_y = (rect.y + rect.height / 2.0) as f64;
    tree.on_event(&press(press_x, press_y));
    tree.on_event(&Event::KeyPressed {
        key: Key::Char('z'),
        modifiers: ModifiersState::default(),
        unmodified: None,
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

    let press_x = (rect.x + TextFieldTokens::current().pad_x() + 2.0) as f64;
    let press_y = (rect.y + rect.height / 2.0) as f64;
    tree.on_event(&press(press_x, press_y));
    tree.on_event(&Event::KeyPressed {
        key: Key::Named(NamedKey::Enter),
        modifiers: ModifiersState::default(),
        unmodified: None,
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

#[test]
fn a_key_hook_hears_keys_first_and_takes_only_the_ones_it_answers() {
    use std::cell::{Cell, RefCell};

    let value = signal("a".to_string());
    let submitted = Rc::new(Cell::new(false));
    let heard = Rc::new(RefCell::new(Vec::new()));
    let (submit, sink) = (submitted.clone(), heard.clone());
    let (field, rect) = laid_out_field(
        TextFieldProps::props()
            .value(value)
            .on_submit(Rc::new(move || submit.set(true)) as Rc<dyn Fn()>)
            .on_key(Rc::new(move |key: &Key, _: ModifiersState| {
                sink.borrow_mut().push(key.clone());
                matches!(key, Key::Named(NamedKey::Enter))
            }))
            .build(),
    );
    let mut tree = ComponentList::new(field);
    let _ = tree.commands();
    tree.on_event(&press(
        (rect.x + TextFieldTokens::current().pad_x() + 2.0) as f64,
        (rect.y + rect.height / 2.0) as f64,
    ));
    for key in [Key::Named(NamedKey::Enter), Key::Char('z')] {
        tree.on_event(&Event::KeyPressed {
            key,
            modifiers: ModifiersState::default(),
            unmodified: None,
        });
    }

    assert!(!submitted.get(), "the hook took Enter");
    assert_eq!(value.get(), "az", "and left the letter to the field");
    assert_eq!(
        *heard.borrow(),
        [Key::Named(NamedKey::Enter), Key::Char('z')]
    );
}

#[test]
fn a_field_points_a_reader_at_the_suggestion_its_cursor_is_on() {
    crate::test_support::fresh_layout_runtime();
    let row = StyledContainer::new(LayoutStyle::new(), |_r| RectStyle::default(), vec![])
        .unwrap()
        .presented(telar::focus::Role::Option);
    let suggestion = row.focus_id().expect("a presented row has an id");
    let (_field, _) = lay_out_field(
        TextFieldProps::props()
            .active_descendant(Rc::new(move || Some(suggestion)))
            .build(),
    );
    let input = telar::focus::exposed()
        .into_iter()
        .filter(|exposed| exposed.role == telar::focus::Role::TextInput)
        .map(|exposed| exposed.id)
        .max()
        .expect("the field is in the tab order");
    assert_eq!(
        telar::focus::active_descendant_state(input),
        Some(suggestion)
    );
}

/// The control is the input, and an input as tall as its bare line (18px at the workbench's 13) was a target too small to be sure of — while the box drawn around it, padding included, was not.
#[test]
fn the_input_is_as_tall_as_the_box_so_the_whole_field_is_the_target() {
    crate::test_support::fresh_layout_runtime();
    telar::focus::clear();
    let field = text_field(TextFieldProps::props().build(), Children::default()).unwrap();
    let root = new_container(
        LayoutStyle::new().flex_column().width(400.0).height(200.0),
        &[field.layout_node()],
    )
    .unwrap();
    telar::declare(root, telar::Declared::default().with_font_size(13.0));
    compute_layout(
        root,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();

    let input = telar::focus::exposed()
        .into_iter()
        .find(|e| e.role == telar::focus::Role::TextInput)
        .expect("the field exposes its input");
    let target = track_layout(input.node).unwrap().get();
    let boxed = track_layout(field.layout_node()).unwrap().get();
    assert!(
        target.height >= shared::MIN_TARGET,
        "the input is a full-size target: {target:?}"
    );
    assert_eq!(
        target.height, boxed.height,
        "and is the whole height of the box: {target:?} in {boxed:?}"
    );
}

fn mounted(props: TextFieldProps) -> ComponentList {
    ComponentList::new(laid_out_field(props).0)
}

#[test]
fn the_corner_is_the_theme_s_control_role_until_the_text_field_tokens_name_one() {
    crate::test_support::assert_corner_follows_the_role_until_the_tokens_name_one(
        || mounted(TextFieldProps::props().build()),
        |tree| crate::test_support::first_painted(tree).radius,
        |_, role| BorderRadius::all(role),
        |radius| TextFieldTokens {
            radius: Some(radius),
            ..TextFieldTokens::default()
        },
    );
}

#[derive(Clone)]
struct Hairline(f32);

impl telar::ThemeTokens for Hairline {
    fn border_width(&self) -> f32 {
        self.0
    }
}

#[test]
fn the_frame_is_the_theme_s_hairline() {
    telar::set_theme(Hairline(3.0));
    let tree = mounted(TextFieldProps::props().build());
    assert_eq!(
        crate::test_support::first_painted(&tree).border,
        Some(Border::uniform(telar::use_theme_tokens().border(), 3.0))
    );
}

#[test]
fn a_style_amendment_reshapes_the_box() {
    let flat: SurfaceAmendment = Rc::new(|s: RectStyle| s.with_radius(BorderRadius::all(0.0)));
    let tree = mounted(TextFieldProps::props().style(flat).build());
    let painted = crate::test_support::first_painted(&tree);
    assert_eq!(painted.radius, BorderRadius::all(0.0));
    assert!(painted.border.is_some(), "the rest of the paint is kept");
}

/// A field with no width of its own takes the theme's, at the control size in force, and so does the caption column around it.
#[test]
fn the_default_width_follows_the_control_size() {
    crate::test_support::fresh_layout_runtime();
    let mut built = None;
    let field = telar::provide_control_size(telar::ControlSize::Large, || {
        built = Some(TextFieldTokens::current().width());
        text_field(
            TextFieldProps::props().label("Name").build(),
            Children::default(),
        )
    })
    .unwrap();
    let rect = track_layout(field.layout_node()).unwrap();
    let root = new_container(
        LayoutStyle::new().flex_column().width(800.0).height(200.0),
        &[field.layout_node()],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(800.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();
    let width = built.expect("the field was built");
    assert_eq!(width, (TextFieldTokens::default().width() * 1.25).round());
    assert_eq!(rect.get().width, width);
}
