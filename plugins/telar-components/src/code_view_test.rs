use std::cell::RefCell;

use telar::testing::{advance_time, centre, press, release, route};
use telar::{
    AvailableSpace, ComponentList, DrawCommand, Paint, compute_layout, focus, new_container,
    relayout_if_dirty,
};

use super::*;

const SOURCE: &str = "fn main() {\n    let x = 1;\n}\n";

fn spans() -> Vec<CodeSpan> {
    vec![
        CodeSpan::new(0..2, TokenKind::Keyword),
        CodeSpan::new(3..7, TokenKind::Function),
        CodeSpan::new(16..19, TokenKind::Keyword),
        CodeSpan::new(24..25, TokenKind::Number),
    ]
}

#[test]
fn tabs_are_set_out_and_runs_move_with_their_text() {
    let prepared = prepare(
        "\tlet a;\r\nx\tb\n",
        &[
            CodeSpan::new(1..4, TokenKind::Keyword),
            CodeSpan::new(11..12, TokenKind::Type),
        ],
    );
    assert_eq!(prepared.text, "    let a;\nx   b");
    assert_eq!(prepared.lines, 2);
    assert_eq!(
        prepared.runs,
        [(4..7, TokenKind::Keyword), (15..16, TokenKind::Type)]
    );
}

#[test]
fn a_run_that_splits_a_character_or_runs_past_the_end_is_left_plain() {
    let prepared = prepare(
        "é",
        &[
            CodeSpan::new(0..1, TokenKind::Keyword),
            CodeSpan::new(0..9, TokenKind::Keyword),
            CodeSpan::new(0..2, TokenKind::String),
        ],
    );
    assert_eq!(prepared.runs, [(0..2, TokenKind::String)]);
}

#[test]
fn empty_code_is_one_empty_line() {
    let prepared = prepare("", &[]);
    assert_eq!((prepared.text.as_str(), prepared.lines), ("", 1));
}

struct Mounted {
    tree: ComponentList,
    node: telar::NodeId,
    copied: Rc<RefCell<Vec<String>>>,
}

impl Mounted {
    fn new(props: CodeViewProps) -> Self {
        crate::test_support::fresh_layout_runtime();
        telar::set_locale("en");
        focus::clear();
        let item = code_view(props, Children::default()).unwrap();
        let node = item.layout_node();
        let root = new_container(
            LayoutStyle::new().flex_column().width(600.0).height(400.0),
            &[node],
        )
        .unwrap();
        compute_layout(
            root,
            AvailableSpace::Definite(600.0),
            AvailableSpace::Definite(400.0),
        )
        .unwrap();
        let mut mounted = Self {
            tree: ComponentList::new(item),
            node,
            copied: Rc::default(),
        };
        mounted.settle();
        mounted
    }

    fn with_copy_sink(props: impl FnOnce(Rc<dyn Fn(&str)>) -> CodeViewProps) -> Self {
        let copied: Rc<RefCell<Vec<String>>> = Rc::default();
        let sink = copied.clone();
        let mut mounted = Self::new(props(Rc::new(move |code: &str| {
            sink.borrow_mut().push(code.to_string())
        })));
        mounted.copied = copied;
        mounted
    }

    fn settle(&mut self) {
        for _ in 0..3 {
            relayout_if_dirty();
            let _ = self.tree.commands();
        }
    }

    fn texts(&self) -> Vec<(String, Option<Vec<Span>>)> {
        self.tree
            .commands()
            .iter()
            .filter_map(|command| match command {
                DrawCommand::Text { text, spans, .. } => {
                    Some((text.to_string(), spans.as_ref().map(|spans| spans.to_vec())))
                }
                _ => None,
            })
            .collect()
    }

    fn button(&self, name: &str) -> Option<telar::AccessNode> {
        ui_core::accessibility::snapshot(&self.tree.commands())
            .into_iter()
            .find(|node| node.role == Role::Button && node.name == name)
    }

    fn height(&self) -> f32 {
        track_layout(self.node).unwrap().get().height
    }
}

fn source_props() -> CodeViewPropsBuilder {
    CodeViewProps::props().code(SOURCE).spans(spans())
}

#[test]
fn the_code_is_drawn_once_with_its_runs_in_their_token_styles() {
    let mounted = Mounted::new(source_props().build());
    let texts = mounted.texts();
    let (_, spans) = texts
        .iter()
        .find(|(text, _)| text == "fn main() {\n    let x = 1;\n}")
        .expect("the code is drawn as one text, its closing newline dropped");
    let spans = spans.as_ref().expect("with its runs");
    let keyword = Paint::Solid(telar::use_theme_tokens().primary());
    let ranges: Vec<_> = spans
        .iter()
        .filter(|span| span.over.color == Some(keyword))
        .map(|span| span.range.clone())
        .collect();
    assert_eq!(ranges, [0..2, 16..19]);
}

#[test]
fn the_gutter_numbers_from_the_first_line_it_is_given() {
    let mounted = Mounted::new(source_props().first_line(41).build());
    assert!(
        mounted.texts().iter().any(|(text, _)| text == "41\n42\n43"),
        "{:?}",
        mounted.texts()
    );
    let bare = Mounted::new(source_props().line_numbers(false).build());
    assert!(!bare.texts().iter().any(|(text, _)| text.starts_with("1\n")));
}

#[test]
fn only_the_marked_lines_that_exist_are_washed() {
    let mounted = Mounted::new(
        source_props()
            .first_line(10)
            .highlighted(vec![11, 99])
            .color(Color::rgb(1.0, 0.0, 0.0))
            .build(),
    );
    let wash = Paint::Solid(Color::rgb(1.0, 0.0, 0.0).with_alpha(MARK_ALPHA));
    let washed = mounted
        .tree
        .commands()
        .iter()
        .filter(|command| {
            matches!(command, DrawCommand::Rect { style, .. } if style.fill == Some(wash))
        })
        .count();
    assert_eq!(washed, 1);
}

#[test]
fn a_caller_can_restyle_a_kind_of_token() {
    let ink = Color::rgb(0.1, 0.8, 0.3);
    let mounted = Mounted::new(
        source_props()
            .token_style(Rc::new(move |kind| match kind {
                TokenKind::Number => Declared::default().with_color(ink),
                _ => Declared::default(),
            }))
            .build(),
    );
    let texts = mounted.texts();
    let spans = texts
        .iter()
        .find_map(|(text, spans)| text.starts_with("fn main").then_some(spans.clone()))
        .flatten()
        .expect("the code's runs");
    let number = spans
        .iter()
        .find(|span| span.range == (24..25))
        .expect("the number's run");
    assert_eq!(number.over.color, Some(Paint::Solid(ink)));
}

#[test]
fn copying_hands_over_the_code_as_written_and_says_so_for_a_while() {
    let mut mounted = Mounted::with_copy_sink(|sink| source_props().on_copy(sink).build());
    let copy = mounted.button("Copy").expect("a copy button");
    let (x, y) = centre(copy.rect);
    route(&mut mounted.tree, &press(x, y));
    route(&mut mounted.tree, &release(x, y));
    mounted.settle();
    assert_eq!(*mounted.copied.borrow(), [SOURCE]);
    assert!(mounted.button("Copied").is_some());

    advance_time(COPIED_FOR * 2);
    mounted.settle();
    assert!(mounted.button("Copy").is_some());
}

#[test]
fn no_copy_button_when_it_is_not_wanted() {
    let mounted = Mounted::new(source_props().copyable(false).build());
    assert!(mounted.button("Copy").is_none());
}

#[test]
fn a_tall_block_stops_at_its_max_height() {
    let tall: String = (0..40).map(|i| format!("line {i}\n")).collect();
    let free = Mounted::new(CodeViewProps::props().code(tall.clone()).build());
    assert!(free.height() > 300.0, "{}", free.height());
    let capped = Mounted::new(CodeViewProps::props().code(tall).max_height(120.0).build());
    assert_eq!(capped.height(), 120.0);
}
