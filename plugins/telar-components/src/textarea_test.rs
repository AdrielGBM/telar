use telar::testing::{key_with, lay_out, named, press, release, route};
use telar::{AccessNode, ComponentList, Key, NamedKey, Role, focus, relayout_if_dirty};

use super::*;

struct Mounted {
    tree: ComponentList,
    value: RwSignal<String>,
}

impl Mounted {
    fn new(props: TextareaPropsBuilder) -> Self {
        crate::test_support::fresh_layout_runtime();
        focus::clear();
        let value = signal(String::new());
        let item = textarea(props.value(value).build(), Children::default()).unwrap();
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
        ui_core::accessibility::snapshot(&self.tree.commands())
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
        TextareaProps::props()
            .label("Notes")
            .placeholder("Say more"),
    );
    let area = mounted.area();
    assert_eq!(area.name, "Notes");
    assert_eq!(crate::test_support::first_painted(&mounted.tree), frame());
}

#[test]
fn an_unlabelled_area_is_called_by_its_placeholder() {
    let mounted = Mounted::new(TextareaProps::props().placeholder("Describe the bug"));
    assert_eq!(mounted.area().name, "Describe the bug");
}

#[test]
fn it_is_rows_lines_tall_and_one_row_stands_as_tall_as_a_field() {
    let three = Mounted::new(TextareaProps::props().rows(3));
    assert!(
        (three.area().rect.height - 3.0 * line()).abs() < 1.0,
        "{} for three lines of {}",
        three.area().rect.height,
        line()
    );
    let one = Mounted::new(TextareaProps::props().rows(1));
    assert!(
        (one.frame_height() - shared::control_height()).abs() < 1.0,
        "{} beside a control height of {}",
        one.frame_height(),
        shared::control_height()
    );
}

#[test]
fn it_grows_with_what_is_typed_up_to_max_rows_and_scrolls_past_it() {
    let mut mounted = Mounted::new(TextareaProps::props().rows(2).max_rows(4));
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
    let mut mounted = Mounted::new(TextareaProps::props().rows(4));
    mounted.click_area();
    assert_eq!(focus::current(), mounted.area().id);
    mounted.type_text("hi");
    assert_eq!(mounted.value.get(), "hi");
}

#[test]
fn the_counter_counts_characters_against_the_limit_and_says_so() {
    let mut mounted = Mounted::new(TextareaProps::props().counter(true).max_length(5));
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
    let mounted = Mounted::new(TextareaProps::props());
    assert!(!telar::testing::find_text(&mounted.tree, "0"));
}

const LONG_LINE: &str =
    "a line long enough to need several rows in a box as narrow as a text field";

#[test]
fn a_long_line_wraps_onto_rows_and_the_box_grows_with_them() {
    let mut mounted = Mounted::new(TextareaProps::props().rows(1).max_rows(6));
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
        TextareaProps::props()
            .rows(1)
            .max_rows(6)
            .text_wrap(TextWrap::NoWrap),
    );
    let rest = mounted.frame_height();
    mounted.click_area();
    mounted.type_text(LONG_LINE);
    assert!((mounted.frame_height() - rest).abs() < 1.0);
}
