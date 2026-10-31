use std::rc::Rc;

use telar::testing::press;
use telar::{
    AvailableSpace, ComponentList, DrawCommand, Event, Key, ModifiersState, NamedKey,
    compute_layout, new_container, track_layout,
};

use super::*;
use crate::test_support::find_text;

// Returns the field and its absolute box rect. Valid only when `label` is empty: a labelled field's outer node is the wrapping column, offset above the box by the caption row.
fn laid_out_field(props: TextFieldProps) -> (Box<dyn LayoutItem>, telar::Rect) {
    crate::test_support::fresh_layout_runtime();
    lay_out_field(props)
}

fn lay_out_field(props: TextFieldProps) -> (Box<dyn LayoutItem>, telar::Rect) {
    let field = text_field(props).unwrap();
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

/// The §1.4 bug, made unwritable: a properties panel that says its region is 11px used to get a field at 14.98 among 11px labels, with no attribute to correct it — `TextFieldProps` has no size.
#[test]
fn a_field_takes_the_size_the_region_around_it_declared() {
    let value = signal("hi".to_string());
    crate::test_support::fresh_layout_runtime();
    let field = text_field(TextFieldProps::props().value(value).build()).unwrap();
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
    let _ = tree.commands();

    // Past the end of what is in it, which is where a press puts the caret.
    let press_x = (rect.x + rect.width - TextFieldTokens::current().pad_x() - 2.0) as f64;
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

    let press_x = (rect.x + rect.width - TextFieldTokens::current().pad_x() - 2.0) as f64;
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
        (rect.x + rect.width - TextFieldTokens::current().pad_x() - 2.0) as f64,
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
    let field = text_field(TextFieldProps::props().build()).unwrap();
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

/// A field is one line however long its value: drawn whole and cut at the box, not wrapped onto a second row that ran out of a box one control tall.
#[test]
fn a_long_value_is_drawn_on_the_field_s_one_line() {
    const LONG: &str =
        "a value far longer than the field it is typed into, which used to wrap onto a second row";
    let value = signal(LONG.to_string());
    let (field, rect) = laid_out_field(TextFieldProps::props().value(value).width(160.0).build());
    let tree = ComponentList::new(field);
    let commands = tree.commands();
    let wrap = commands
        .iter()
        .find_map(|c| match c {
            DrawCommand::Text { text, style, .. } if text.as_ref() == LONG => Some(style.text_wrap),
            _ => None,
        })
        .expect("the field drew its value");
    assert_eq!(wrap, TextWrap::NoWrap);
    assert!(
        commands
            .iter()
            .any(|c| matches!(c, DrawCommand::PushClip { .. })),
        "and cut it at the box"
    );
    assert_eq!(rect.height, shared::control_height());
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
        text_field(TextFieldProps::props().label("Name").build())
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

/// A field of several lines: the same frame around a `TextArea`, sized by its rows.
mod multiline {
    use telar::testing::{key_with, lay_out, named, press, release, route};
    use telar::{AccessNode, ComponentList, Key, NamedKey, Role, focus, relayout_if_dirty};

    use super::*;
    use crate::shared;

    struct Mounted {
        tree: ComponentList,
        value: RwSignal<String>,
    }

    impl Mounted {
        fn new(props: TextFieldPropsBuilder) -> Self {
            crate::test_support::fresh_layout_runtime();
            focus::clear();
            let value = signal(String::new());
            let item = text_field(props.multiline(true).value(value).build()).unwrap();
            lay_out(item.layout_node(), 400.0, 400.0);
            let tree = ComponentList::new(item);
            let mut mounted = Self { tree, value };
            mounted.settle();
            mounted
        }

        fn settle(&mut self) {
            relayout_if_dirty();
            let _ = self.tree.commands();
            relayout_if_dirty();
            let _ = self.tree.commands();
        }

        fn read(&self) -> Vec<AccessNode> {
            telar::inspect::snapshot(&self.tree.commands())
        }

        fn area(&self) -> AccessNode {
            self.read()
                .into_iter()
                .find(|node| node.role == Role::MultilineTextInput)
                .expect("a multi-line field")
        }

        fn frame_height(&self) -> f32 {
            let area = self.area().rect;
            let painted = self
                .tree
                .commands()
                .iter()
                .find_map(|command| match command {
                    telar::DrawCommand::Rect { rect, .. } if rect.width > area.width => Some(*rect),
                    _ => None,
                })
                .expect("the frame is painted");
            painted.height
        }

        fn type_text(&mut self, text: &str) {
            for c in text.chars() {
                let event = match c {
                    '\n' => named(NamedKey::Enter),
                    c => key_with(Key::Char(c), Default::default()),
                };
                route(&mut self.tree, &event);
                self.settle();
            }
        }

        fn click_area(&mut self) {
            let rect = self.area().rect;
            let (x, y) = (
                f64::from(rect.x + rect.width / 2.0),
                f64::from(rect.y + rect.height - 2.0),
            );
            route(&mut self.tree, &press(x, y));
            route(&mut self.tree, &release(x, y));
            self.settle();
        }
    }

    fn line() -> f32 {
        telar::line_box(&shared::control_text(telar::Inherited::initial().text, 1.0))
    }

    #[test]
    fn a_text_area_is_a_named_multi_line_field_in_a_text_field_s_frame() {
        let mounted = Mounted::new(
            TextFieldProps::props()
                .label("Notes")
                .placeholder("Say more"),
        );
        let area = mounted.area();
        assert_eq!(area.name, "Notes");
        assert_eq!(crate::test_support::first_painted(&mounted.tree), frame());
    }

    #[test]
    fn an_unlabelled_area_is_called_by_its_placeholder() {
        let mounted = Mounted::new(TextFieldProps::props().placeholder("Describe the bug"));
        assert_eq!(mounted.area().name, "Describe the bug");
    }

    #[test]
    fn it_is_rows_lines_tall_and_one_row_stands_as_tall_as_a_field() {
        let three = Mounted::new(TextFieldProps::props().rows(3));
        assert!(
            (three.area().rect.height - 3.0 * line()).abs() < 1.0,
            "{} for three lines of {}",
            three.area().rect.height,
            line()
        );
        let one = Mounted::new(TextFieldProps::props().rows(1));
        assert!(
            (one.frame_height() - shared::control_height()).abs() < 1.0,
            "{} beside a control height of {}",
            one.frame_height(),
            shared::control_height()
        );
    }

    #[test]
    fn it_grows_with_what_is_typed_up_to_max_rows_and_scrolls_past_it() {
        let mut mounted = Mounted::new(TextFieldProps::props().rows(2).max_rows(4));
        let rest = mounted.frame_height();
        mounted.click_area();
        mounted.type_text("one\ntwo\nthree");
        let grown = mounted.frame_height();
        assert!(
            (grown - rest - line()).abs() < 1.0,
            "a third line grows the box by one: {rest} → {grown}"
        );
        mounted.type_text("\nfour\nfive\nsix");
        let capped = mounted.frame_height();
        assert!(
            (capped - rest - 2.0 * line()).abs() < 1.0,
            "and it stops at four: {rest} → {capped}"
        );
        assert_eq!(mounted.value.get(), "one\ntwo\nthree\nfour\nfive\nsix");
    }

    #[test]
    fn a_press_low_in_an_empty_area_still_lands_in_it() {
        let mut mounted = Mounted::new(TextFieldProps::props().rows(4));
        mounted.click_area();
        assert_eq!(focus::current(), mounted.area().id);
        mounted.type_text("hi");
        assert_eq!(mounted.value.get(), "hi");
    }

    #[test]
    fn the_counter_counts_characters_against_the_limit_and_says_so() {
        let mut mounted = Mounted::new(TextFieldProps::props().counter(true).max_length(5));
        assert!(telar::testing::find_text(&mounted.tree, "0 / 5"));
        mounted.value.set("héllo!".to_string());
        mounted.settle();
        assert!(telar::testing::find_text(&mounted.tree, "6 / 5"));
        assert!(
            mounted
                .read()
                .iter()
                .any(|node| node.name == "6 of 5 characters"),
            "a reader hears the count in words"
        );
        let over = mounted
            .tree
            .commands()
            .iter()
            .find_map(|command| match command {
                telar::DrawCommand::Text { text, style, .. } if text.as_ref() == "6 / 5" => {
                    Some(style.color.solid_color())
                }
                _ => None,
            })
            .expect("the count is drawn");
        assert_eq!(over, use_theme_tokens().error());
    }

    #[test]
    fn without_a_counter_nothing_is_counted() {
        let mounted = Mounted::new(TextFieldProps::props());
        assert!(!telar::testing::find_text(&mounted.tree, "0"));
    }

    const LONG_LINE: &str =
        "a line long enough to need several rows in a box as narrow as a text field";

    #[test]
    fn a_long_line_wraps_onto_rows_and_the_box_grows_with_them() {
        let mut mounted = Mounted::new(TextFieldProps::props().rows(1).max_rows(6));
        let rest = mounted.frame_height();
        let width = mounted.area().rect.width;
        mounted.click_area();
        mounted.type_text(LONG_LINE);
        let grown = mounted.frame_height();
        let rows = ((grown - rest) / line()).round() + 1.0;
        assert!(rows >= 2.0, "{rest} → {grown}");
        assert!(
            (grown - rest - (rows - 1.0) * line()).abs() < 1.0,
            "the box grows a whole row at a time: {rest} → {grown}"
        );
        assert_eq!(mounted.area().rect.width, width, "and never wider");
        assert_eq!(mounted.value.get(), LONG_LINE, "a wrap is no newline");
    }

    #[test]
    fn an_area_that_does_not_wrap_keeps_a_long_line_on_one_row() {
        let mut mounted = Mounted::new(
            TextFieldProps::props()
                .rows(1)
                .max_rows(6)
                .text_wrap(TextWrap::NoWrap),
        );
        let rest = mounted.frame_height();
        mounted.click_area();
        mounted.type_text(LONG_LINE);
        assert!((mounted.frame_height() - rest).abs() < 1.0);
    }
}
