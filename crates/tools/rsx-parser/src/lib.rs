//! Hand-written parser for `.rsx` source files (Proposal 4 syntax).
//!
//! A `.rsx` file has three sections:
//! - a leading **logic** zone of verbatim Rust,
//! - a `[style]` section of constants and style classes,
//! - a `[view]` section describing an indentation-based node tree.
//!
//! [`parse`] turns the source into an [`RsxDocument`] AST consumed by the transpiler.

mod ast;
mod error;
mod lexer;
mod parser;

pub use ast::*;
pub use error::ParseError;
pub use lexer::{Section, header_section};

/// Parses `.rsx` source text into an [`RsxDocument`].
pub fn parse(source: &str) -> Result<RsxDocument, ParseError> {
    parser::Parser::new(source).parse()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"[logic]
use rsx::prelude::*;
let count = create_rw_signal(0i32);
#[derive(Props)]
pub struct Props {
    pub title: &'static str,
}
fn reset() { count.set(0); }

[style]

primary:  #3d78fa
danger:   #eb4444
dark:     #141424
muted:    #808098

.card
    width:    240
    padding:  20
    gap:      12
    direction: col
    align:    center

.badge: padding-x:6  padding-y:2  radius:6

[view]

col .card
    text "Count: {count}"   size:14  color:dark
    text "Double: {double}" size:12  color:muted
    row  gap:8
        btn "Increment"  fill:primary   on_press:|| count.update(|n| *n += 1)
        btn "Decrement"  outline:danger  on_press:|| count.update(|n| *n -= 1)
        btn "Reset"      ghost           on_press:|| reset()
"#;

    #[test]
    fn parses_logic_zone_verbatim() {
        let doc = parse(SAMPLE).unwrap();
        assert!(doc.logic.source.starts_with("use rsx::prelude::*;"));
        assert!(doc.logic.source.contains("fn reset() { count.set(0); }"));
        assert!(doc.logic.source.contains("#[derive(Props)]"));
        // The section header must not leak into the logic zone.
        assert!(!doc.logic.source.contains("[style]"));
    }

    #[test]
    fn parses_style_constants() {
        let doc = parse(SAMPLE).unwrap();
        assert_eq!(doc.style.constants.len(), 4);
        let primary = &doc.style.constants[0];
        assert_eq!(primary.name, "primary");
        assert_eq!(primary.value, StyleValue::Hex("#3d78fa".to_string()));
    }

    #[test]
    fn parses_multiline_style_class() {
        let doc = parse(SAMPLE).unwrap();
        let card = doc
            .style
            .classes
            .iter()
            .find(|c| c.name == "card")
            .expect("card class");
        assert_eq!(card.props.len(), 5);
        assert_eq!(
            card.props[0],
            StyleProp {
                key: "width".into(),
                value: "240".into()
            }
        );
        assert_eq!(
            card.props[3],
            StyleProp {
                key: "direction".into(),
                value: "col".into()
            }
        );
    }

    #[test]
    fn parses_inline_style_class() {
        let doc = parse(SAMPLE).unwrap();
        let badge = doc
            .style
            .classes
            .iter()
            .find(|c| c.name == "badge")
            .expect("badge class");
        assert_eq!(badge.props.len(), 3);
        assert_eq!(badge.props[0].key, "padding-x");
        assert_eq!(badge.props[0].value, "6");
        assert_eq!(badge.props[2].key, "radius");
        assert_eq!(badge.props[2].value, "6");
    }

    #[test]
    fn parses_view_tree_with_indentation() {
        let doc = parse(SAMPLE).unwrap();
        assert_eq!(doc.view.nodes.len(), 1);

        let ViewNode::Element(col) = &doc.view.nodes[0] else {
            panic!("root should be an element");
        };
        assert_eq!(col.tag, "col");
        assert_eq!(col.classes, vec!["card".to_string()]);
        // two text nodes + one row
        assert_eq!(col.children.len(), 3);

        let ViewNode::Element(text0) = &col.children[0] else {
            panic!("expected text element");
        };
        assert_eq!(text0.tag, "text");
        assert_eq!(text0.content.as_deref(), Some("Count: {count}"));
        assert_eq!(text0.attrs.len(), 2);
        assert_eq!(
            text0.attrs[0],
            Attr {
                key: "size".into(),
                value: "14".into(),
                is_quoted: false
            }
        );
        assert_eq!(
            text0.attrs[1],
            Attr {
                key: "color".into(),
                value: "dark".into(),
                is_quoted: false
            }
        );

        let ViewNode::Element(row) = &col.children[2] else {
            panic!("expected row element");
        };
        assert_eq!(row.tag, "row");
        assert_eq!(
            row.attrs[0],
            Attr {
                key: "gap".into(),
                value: "8".into(),
                is_quoted: false
            }
        );
        assert_eq!(row.children.len(), 3);
    }

    #[test]
    fn parses_closure_attribute_to_end_of_line() {
        let doc = parse(SAMPLE).unwrap();
        let ViewNode::Element(col) = &doc.view.nodes[0] else {
            panic!();
        };
        let ViewNode::Element(row) = &col.children[2] else {
            panic!();
        };

        let ViewNode::Element(inc) = &row.children[0] else {
            panic!();
        };
        assert_eq!(inc.tag, "btn");
        assert_eq!(inc.content.as_deref(), Some("Increment"));
        assert_eq!(
            inc.attrs[0],
            Attr {
                key: "fill".into(),
                value: "primary".into(),
                is_quoted: false
            }
        );
        let on_press = inc.attrs.iter().find(|a| a.key == "on_press").unwrap();
        assert_eq!(on_press.value, "|| count.update(|n| *n += 1)");
    }

    #[test]
    fn parses_flag_attribute() {
        let doc = parse(SAMPLE).unwrap();
        let ViewNode::Element(col) = &doc.view.nodes[0] else {
            panic!();
        };
        let ViewNode::Element(row) = &col.children[2] else {
            panic!();
        };
        let ViewNode::Element(reset) = &row.children[2] else {
            panic!();
        };
        // `ghost` is a bare flag attribute with an empty value.
        assert!(
            reset
                .attrs
                .iter()
                .any(|a| a.key == "ghost" && a.value.is_empty())
        );
        let on_press = reset.attrs.iter().find(|a| a.key == "on_press").unwrap();
        assert_eq!(on_press.value, "|| reset()");
    }

    #[test]
    fn parses_if_else_block() {
        let src = "[logic]\n[view]\ncol\n    if count > 0\n        text \"positive\"\n    else\n        text \"zero\"\n";
        let doc = parse(src).unwrap();
        let ViewNode::Element(col) = &doc.view.nodes[0] else {
            panic!();
        };
        let ViewNode::IfBlock(if_block) = &col.children[0] else {
            panic!("expected if block");
        };
        assert_eq!(if_block.condition, "count > 0");
        assert_eq!(if_block.then_branch.len(), 1);
        let else_branch = if_block.else_branch.as_ref().expect("else branch");
        assert_eq!(else_branch.len(), 1);
    }

    #[test]
    fn parses_for_block() {
        let src = "[logic]\n[view]\ncol\n    for (i, item) in items.iter().enumerate()\n        text \"{item}\"\n";
        let doc = parse(src).unwrap();
        let ViewNode::Element(col) = &doc.view.nodes[0] else {
            panic!();
        };
        let ViewNode::ForBlock(for_block) = &col.children[0] else {
            panic!("expected for block");
        };
        assert_eq!(for_block.pattern, "(i, item)");
        assert_eq!(for_block.iterable, "items.iter().enumerate()");
        assert_eq!(for_block.body.len(), 1);
    }

    #[test]
    fn parses_let_statement_in_view() {
        let src = "[logic]\n[view]\ncol\n    let bar_w = (w - 16.0) / 2.0\n    text \"x\"\n";
        let doc = parse(src).unwrap();
        let ViewNode::Element(col) = &doc.view.nodes[0] else {
            panic!();
        };
        let ViewNode::LetStmt { source, .. } = &col.children[0] else {
            panic!("expected let statement");
        };
        assert_eq!(source, "let bar_w = (w - 16.0) / 2.0");
    }

    #[test]
    fn parses_canvas_with_params() {
        let src = "[logic]\n[view]\ncanvas .chart\n    |w, h|\n    rect\n";
        let doc = parse(src).unwrap();
        let ViewNode::Element(canvas) = &doc.view.nodes[0] else {
            panic!();
        };
        assert_eq!(canvas.tag, "canvas");
        assert_eq!(canvas.classes, vec!["chart".to_string()]);
        assert_eq!(canvas.canvas_params.as_deref(), Some("w, h"));
        // The `|w, h|` line is consumed, leaving the rect as the only child.
        assert_eq!(canvas.children.len(), 1);
        let ViewNode::Element(rect) = &canvas.children[0] else {
            panic!();
        };
        assert_eq!(rect.tag, "rect");
    }

    #[test]
    fn parses_named_closure_param_attribute() {
        let src = "[logic]\n[view]\nbtn \"x\" on_press:|ev| handle(ev)\n";
        let doc = parse(src).unwrap();
        let ViewNode::Element(btn) = &doc.view.nodes[0] else {
            panic!();
        };
        let on_press = btn.attrs.iter().find(|a| a.key == "on_press").unwrap();
        assert_eq!(on_press.value, "|ev| handle(ev)");
    }

    #[test]
    fn parses_number_style_constant() {
        let src = "[logic]\n[style]\nradius: 6\nlabel: hello\n[view]\ncol\n";
        let doc = parse(src).unwrap();
        assert_eq!(doc.style.constants[0].value, StyleValue::Number(6.0));
        assert_eq!(
            doc.style.constants[1].value,
            StyleValue::Raw("hello".into())
        );
    }

    #[test]
    fn empty_document_is_valid() {
        let doc = parse("").unwrap();
        assert!(doc.logic.source.is_empty());
        assert!(doc.style.constants.is_empty());
        assert!(doc.view.nodes.is_empty());
    }

    #[test]
    fn document_without_logic_zone() {
        let src = "[logic]\n[view]\ntext \"hello\"\n";
        let doc = parse(src).unwrap();
        assert!(doc.logic.source.is_empty());
        assert_eq!(doc.view.nodes.len(), 1);
    }
}
