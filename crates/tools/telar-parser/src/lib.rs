//! Hand-written parser for `.rsx` source files (Proposal 4 syntax).
//!
//! A `.rsx` file has three sections:
//! - a leading **logic** zone of verbatim Rust (a `pub struct Props` here declares the component's props),
//! - a `[style]` section of style classes,
//! - a `[view]` section describing an indentation-based node tree.
//!
//! [`parse`] turns the source into an [`RsxDocument`] AST consumed by the transpiler.
//!
//! # An attribute's value
//!
//! Two rules, and no exceptions to them:
//!
//! - **`key:value`** is one token. Parentheses may nest inside it, so `fill:chip(a, b)` and
//!   `fill:linear(horizontal, a, b)` read whole; a space at depth 0 ends the value.
//! - **`key(…)`** is anything that needs a space: `cols(240 1fr)`, `stroke_width(0 0 1 0)`,
//!   `transition(fill 250ms ease-out)`, `drag_button(secondary auxiliary)`.
//!
//! The parser records *which of the five forms* it read — bare, quoted, `t"…"` for a lookup, a closure, a
//! parenthesised spec — in [`Value`], and does not learn what any of them mean. Meaning belongs to the key
//! schema in `telar-transpiler`'s `registry`, which also holds the closed keyword sets: a value outside one
//! is a build error on the attribute's own line rather than a property silently dropped.
//!
//! Values used to arrive here as bare strings and have their form re-derived downstream — by the transpiler,
//! the analyzer and the formatter separately, each with its own `starts_with('#')` and `contains('(')`. Four
//! unit dialects, three behaviours for a bad value and one run-to-end-of-line parser exception grew in the
//! gap between those re-derivations.
//!
//! [`format::format_document`] is the inverse: it re-serializes that AST into canonical source. It lives beside
//! the parser rather than with the language server so anything holding a `.rsx` file can reach it — the editor
//! through the LSP, `cargo telar fmt` from a terminal — and both give the same answer by construction.

pub mod format;

mod ast;
mod color;
mod error;
mod lexer;
mod parser;

pub use ast::*;
pub use color::parse_hex;
pub use error::ParseError;
pub use lexer::{Section, find_section_at, header_section, is_preview_header};

/// Parses `.rsx` source text into an [`RsxDocument`].
pub fn parse(source: &str) -> Result<RsxDocument, ParseError> {
    parser::Parser::new(source).parse()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"[logic]
use telar::prelude::*;
let count = signal(0i32);
#[derive(Props)]
pub struct Props {
    pub title: &'static str,
}
fn reset() { count.set(0); }

[style]

@card
    width:    240
    padding:  20
    gap:      12
    direction: col
    align:    center

@badge: padding_x:6  padding_y:2  radius:6

[view]

col @card
    text "Count: {count}"   font_size:14  color:dark
    text "Double: {double}" font_size:12  color:muted
    row  gap:8
        btn "Increment"  fill:primary   on_press:(|| count.update(|n| *n += 1))
        btn "Decrement"  outline:danger  on_press:(|| count.update(|n| *n -= 1))
        btn "Reset"      ghost           on_press:(|| reset())
"#;

    #[test]
    fn parses_logic_zone_verbatim() {
        let doc = parse(SAMPLE).unwrap();
        assert!(doc.logic.source.starts_with("use telar::prelude::*;"));
        assert!(doc.logic.source.contains("fn reset() { count.set(0); }"));
        assert!(doc.logic.source.contains("#[derive(Props)]"));
        // The section header must not leak into the logic zone.
        assert!(!doc.logic.source.contains("[style]"));
    }

    #[test]
    fn closure_attribute_requires_parenthesized_form() {
        // The colon form (`on_press:|| …`) ran to end of line and swallowed the attributes after it, so it is rejected outright.
        let err = parse("[view]\nbtn \"x\" on_press:|| f() foo:bar\n").unwrap_err();
        assert!(err.message.contains("parenthesise it"), "{}", err.message);
        // The parenthesized form is delimited, so a trailing attribute can follow it on the same line.
        assert!(parse("[view]\nbtn \"x\" on_press:(|| f()) foo:bar\n").is_ok());
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
        assert_eq!(badge.props[0].key, "padding_x");
        assert_eq!(badge.props[0].value, "6");
        assert_eq!(badge.props[2].key, "radius");
        assert_eq!(badge.props[2].value, "6");
    }

    #[test]
    fn parses_raw_strings_without_escaping() {
        let doc = parse("[view]\ntext r\"a\\b c\" title:r\"C:\\x\"\n").unwrap();
        let ViewNode::Element(text) = &doc.view.nodes[0] else {
            panic!("text element");
        };
        assert_eq!(
            text.content.as_deref(),
            Some("a\\b c"),
            "raw content keeps the backslash literal"
        );
        let title = text.attributes.iter().find(|a| a.key == "title").unwrap();
        assert_eq!(
            title.value,
            Value::Quoted("C:\\x".to_string()),
            "raw attr value keeps the backslash literal, and reads back as a quoted value"
        );
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
        assert_eq!(col.children.len(), 3);

        let ViewNode::Element(text0) = &col.children[0] else {
            panic!("expected text element");
        };
        assert_eq!(text0.tag, "text");
        assert_eq!(text0.content.as_deref(), Some("Count: {count}"));
        assert_eq!(text0.attributes.len(), 2);
        assert_eq!(
            text0.attributes[0],
            Attr {
                key: "font_size".into(),
                value: Value::Expr("14".into()),
                value_start: 0
            }
        );
        assert_eq!(
            text0.attributes[1],
            Attr {
                key: "color".into(),
                value: Value::Expr("dark".into()),
                value_start: 0
            }
        );

        let ViewNode::Element(row) = &col.children[2] else {
            panic!("expected row element");
        };
        assert_eq!(row.tag, "row");
        assert_eq!(
            row.attributes[0],
            Attr {
                key: "gap".into(),
                value: Value::Expr("8".into()),
                value_start: 0
            }
        );
        assert_eq!(row.children.len(), 3);
    }

    #[test]
    fn parses_closure_attribute() {
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
            inc.attributes[0],
            Attr {
                key: "fill".into(),
                value: Value::Expr("primary".into()),
                value_start: 0
            }
        );
        let on_press = inc.attributes.iter().find(|a| a.key == "on_press").unwrap();
        assert_eq!(
            on_press.value,
            Value::Expr("(|| count.update(|n| *n += 1))".into())
        );
    }

    #[test]
    fn a_parenthesized_value_keeps_its_spaces_and_commas() {
        // `key(…)` is the one form that admits a space at depth 0, so a multi-clause spec survives tokenization and the attributes on either side of it still parse as their own.
        let doc = parse(
            "[view]\nbox fill:primary transition(opacity 200ms ease-out, fill 150ms linear) radius:6\n",
        )
        .unwrap();
        let ViewNode::Element(b) = &doc.view.nodes[0] else {
            panic!("root should be an element");
        };
        let fill = b.attributes.iter().find(|a| a.key == "fill").unwrap();
        assert_eq!(fill.value, Value::Expr("primary".into()));
        let transition = b.attributes.iter().find(|a| a.key == "transition").unwrap();
        assert_eq!(
            transition.value,
            Value::Directive("opacity 200ms ease-out, fill 150ms linear".into())
        );
        let radius = b.attributes.iter().find(|a| a.key == "radius").unwrap();
        assert_eq!(radius.value, Value::Expr("6".into()));
    }

    #[test]
    fn colon_value_keeps_spaces_inside_parens() {
        // A computed colon value balances parens: `fill:chip_fill($snap, id)` is read whole (the space after
        // the comma is nested inside `(...)`), and a following attribute on the same line still parses.
        let doc = parse(
            "[view]\nbox fill:chip_fill($snap, id) radius:6\n    text \"x\" color:text_color($snap, id) font_size:13\n",
        )
        .unwrap();
        let ViewNode::Element(b) = &doc.view.nodes[0] else {
            panic!("root should be an element");
        };
        let fill = b.attributes.iter().find(|a| a.key == "fill").unwrap();
        assert_eq!(fill.value, Value::Expr("chip_fill($snap, id)".into()));
        let radius = b.attributes.iter().find(|a| a.key == "radius").unwrap();
        assert_eq!(
            radius.value,
            Value::Expr("6".into()),
            "the trailing attribute still parses"
        );
        let ViewNode::Element(t) = &b.children[0] else {
            panic!("child should be a text element");
        };
        let color = t.attributes.iter().find(|a| a.key == "color").unwrap();
        assert_eq!(color.value, Value::Expr("text_color($snap, id)".into()));
        let size = t.attributes.iter().find(|a| a.key == "font_size").unwrap();
        assert_eq!(size.value, Value::Expr("13".into()));
    }

    /// `transition:` was the one key whose colon value ran to end of line, which is also why it needed a
    /// bespoke check for the attributes that run swallowed. It obeys the one rule now, so nothing is
    /// swallowed and nothing has to be last.
    #[test]
    fn a_colon_value_ends_at_the_first_space() {
        let doc = parse("[view]\nbox transition:opacity align:center\n").unwrap();
        let ViewNode::Element(b) = &doc.view.nodes[0] else {
            panic!();
        };
        let transition = b.attributes.iter().find(|a| a.key == "transition").unwrap();
        assert_eq!(transition.value, Value::Expr("opacity".into()));
        let align = b.attributes.iter().find(|a| a.key == "align").unwrap();
        assert_eq!(align.value, Value::Expr("center".into()));
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
                .attributes
                .iter()
                .any(|a| a.key == "ghost" && a.value.is_flag())
        );
        let on_press = reset
            .attributes
            .iter()
            .find(|a| a.key == "on_press")
            .unwrap();
        assert_eq!(on_press.value, Value::Expr("(|| reset())".into()));
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

    /// `else if` used to match the plain-`else` check and have the rest of its line thrown away, silently — the
    /// branch ran unconditionally and nothing said so. It chains now, and the absence of this test is why the
    /// fault survived.
    #[test]
    fn parses_an_else_if_chain() {
        let src = "[logic]\n[view]\ncol\n    if n > 1\n        text \"many\"\n    else if n > 0\n        text \"one\"\n    else\n        text \"none\"\n";
        let doc = parse(src).unwrap();
        let ViewNode::Element(col) = &doc.view.nodes[0] else {
            panic!();
        };
        let ViewNode::IfBlock(outer) = &col.children[0] else {
            panic!("expected if block");
        };
        assert_eq!(outer.condition, "n > 1");
        let else_branch = outer.else_branch.as_ref().expect("else branch");
        assert_eq!(else_branch.len(), 1, "the chain nests, it does not flatten");
        let ViewNode::IfBlock(inner) = &else_branch[0] else {
            panic!("the else branch holds the chained if");
        };
        assert_eq!(inner.condition, "n > 0");
        assert_eq!(
            inner.else_branch.as_ref().expect("trailing else").len(),
            1,
            "and the trailing else belongs to the innermost if"
        );
        // The condition still points at its own bytes, so a diagnostic on it lands on the right column.
        assert_eq!(&src[inner.condition_start..][.."n > 0".len()], "n > 0");
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
        assert_eq!(for_block.key_expr, None);
        assert_eq!(for_block.gap_expr, None);
    }

    #[test]
    fn parses_reactive_for_key_and_gap_clauses() {
        let src = "[logic]\n[view]\ncol\n    for item in $items key item.id gap:8\n        text \"{item}\"\n";
        let doc = parse(src).unwrap();
        let ViewNode::Element(col) = &doc.view.nodes[0] else {
            panic!();
        };
        let ViewNode::ForBlock(for_block) = &col.children[0] else {
            panic!("expected for block");
        };
        assert_eq!(for_block.iterable, "$items");
        assert_eq!(for_block.key_expr.as_deref(), Some("item.id"));
        assert_eq!(for_block.gap_expr.as_deref(), Some("8"));
    }

    #[test]
    fn parses_reactive_for_with_gap_but_no_key() {
        let src = "[logic]\n[view]\ncol\n    for item in $items gap:8\n        text \"{item}\"\n";
        let doc = parse(src).unwrap();
        let ViewNode::Element(col) = &doc.view.nodes[0] else {
            panic!();
        };
        let ViewNode::ForBlock(for_block) = &col.children[0] else {
            panic!("expected for block");
        };
        assert_eq!(for_block.iterable, "$items");
        assert_eq!(for_block.key_expr, None);
        assert_eq!(for_block.gap_expr.as_deref(), Some("8"));
    }

    #[test]
    fn parses_let_statement_in_view() {
        let src = "[logic]\n[view]\ncol\n    let bar_w = (w - 16.0) / 2.0\n    text \"x\"\n";
        let doc = parse(src).unwrap();
        let ViewNode::Element(col) = &doc.view.nodes[0] else {
            panic!();
        };
        let ViewNode::LetStmt(stmt) = &col.children[0] else {
            panic!("expected let statement");
        };
        assert_eq!(stmt.source, "let bar_w = (w - 16.0) / 2.0");
    }

    #[test]
    fn parses_named_closure_param_attribute() {
        let src = "[logic]\n[view]\nbtn \"x\" on_press:(|ev| handle(ev))\n";
        let doc = parse(src).unwrap();
        let ViewNode::Element(btn) = &doc.view.nodes[0] else {
            panic!();
        };
        let on_press = btn.attributes.iter().find(|a| a.key == "on_press").unwrap();
        assert_eq!(on_press.value, Value::Expr("(|ev| handle(ev))".into()));
    }

    #[test]
    fn empty_document_is_valid() {
        let doc = parse("").unwrap();
        assert!(doc.logic.source.is_empty());
        assert!(doc.style.classes.is_empty());
        assert!(doc.view.nodes.is_empty());
    }

    #[test]
    fn document_without_logic_zone() {
        let src = "[logic]\n[view]\ntext \"hello\"\n";
        let doc = parse(src).unwrap();
        assert!(doc.logic.source.is_empty());
        assert_eq!(doc.view.nodes.len(), 1);
    }

    #[test]
    fn inconsistent_indentation_errors_instead_of_dropping_nodes() {
        // The first child sets the child indent (5); the next two siblings sit at 4, which lines up
        // with no enclosing block. This used to parse Ok while silently discarding them.
        let src = "[view]\nbox\n     text \"a\"\n    text \"b\"\n    text \"c\"\n";
        let err = parse(src).unwrap_err();
        // The error points at the first stranded line so the editor can flag it.
        assert_eq!(err.line, 4);
        assert!(err.message.contains("indentation"));
    }

    #[test]
    fn consistent_sibling_indentation_keeps_all_children() {
        let src = "[view]\nbox\n    text \"a\"\n    text \"b\"\n    text \"c\"\n";
        let doc = parse(src).unwrap();
        let ViewNode::Element(box_el) = &doc.view.nodes[0] else {
            panic!();
        };
        assert_eq!(box_el.children.len(), 3);
    }

    #[test]
    fn view_attribute_bad_hex_errors_but_valid_passes() {
        let err = parse("[view]\nbox fill:#zz\n").unwrap_err();
        assert_eq!(err.line, 2);
        assert!(err.message.contains("invalid hex"));
        // An 8-digit hex (as real files use for shadow_color) is accepted.
        assert!(parse("[view]\nbox shadow_color:#00000040\n").is_ok());
        // A quoted value that happens to start with `#` is a string, not a color — not validated.
        assert!(parse("[view]\ntext label:\"#hashtag\"\n").is_ok());
    }

    #[test]
    fn style_class_prop_empty_or_bad_hex_errors() {
        // Empty value in a multi-line class prop.
        let err = parse("[style]\n@card\n    width:\n[view]\ncol\n").unwrap_err();
        assert_eq!(err.line, 3);
        assert!(err.message.contains("missing a value"));
        // Bad hex in an inline class prop.
        let err = parse("[style]\n@card: bg:#zz\n[view]\ncol\n").unwrap_err();
        assert_eq!(err.line, 2);
        assert!(err.message.contains("invalid hex"));
    }

    #[test]
    fn invalid_hex_errors_but_valid_hex_parses() {
        for bad in ["#zzz", "#12", "#12345", "#1234567"] {
            let src = format!("[style]\n@card\n    fill: {bad}\n[view]\ncol\n");
            let err = parse(&src).unwrap_err();
            assert!(
                err.message.contains("invalid hex"),
                "expected reject of {bad}"
            );
        }
        // `#abcd` is the four-digit form `Color::from_hex` has always accepted at runtime; rejecting it here
        // made a legal colour a parse error.
        for good in ["#abc", "#abcd", "#3d78fa", "#3d78fa80"] {
            let src = format!("[style]\n@card\n    fill: {good}\n[view]\ncol\n");
            assert!(parse(&src).is_ok(), "expected accept of {good}");
        }
    }

    #[test]
    fn control_flow_nodes_carry_their_source_line() {
        let src =
            "[view]\ncol\n    if flag\n        text \"a\"\n    for x in xs\n        text \"{x}\"\n";
        let doc = parse(src).unwrap();
        let ViewNode::Element(col) = &doc.view.nodes[0] else {
            panic!();
        };
        let ViewNode::IfBlock(if_block) = &col.children[0] else {
            panic!();
        };
        assert_eq!(if_block.line, 3);
        let ViewNode::ForBlock(for_block) = &col.children[1] else {
            panic!();
        };
        assert_eq!(for_block.line, 5);
    }

    #[test]
    fn parses_preview_sections() {
        let src = "[logic]\n[view]\ncol\n    text \"x\"\n\n[preview \"Default\"]\ncounter\n\n[preview \"Tall\" width:360 dark]\nbox\n    text \"hi\"\n";
        let doc = parse(src).unwrap();
        // The `[view]` section is unaffected by the trailing previews.
        assert_eq!(doc.view.nodes.len(), 1);
        assert_eq!(doc.previews.len(), 2);

        // A preview body is ordinary view markup (here, a bare component call).
        assert_eq!(doc.previews[0].name, "Default");
        assert!(doc.previews[0].options.is_empty());
        let ViewNode::Element(comp) = &doc.previews[0].body[0] else {
            panic!("preview body should be a view element");
        };
        assert_eq!(comp.tag, "counter");

        // Options parse as `key:value` pairs and bare flags (empty value).
        assert_eq!(doc.previews[1].name, "Tall");
        assert_eq!(
            doc.previews[1].options,
            vec![
                StyleProp {
                    key: "width".to_string(),
                    value: "360".to_string(),
                },
                StyleProp {
                    key: "dark".to_string(),
                    value: String::new(),
                },
            ]
        );
        let ViewNode::Element(b) = &doc.previews[1].body[0] else {
            panic!();
        };
        assert_eq!(b.tag, "box");
        assert_eq!(b.children.len(), 1);
    }
    /// A value whose delimiters are still open at end of line continues onto the next, so a closure can be
    /// written where it is used instead of being bound in `[logic]` and referred to by name. Without this a
    /// `canvas` could never carry its own drawing, and the only way to place one was the `widget` escape.
    #[test]
    fn a_value_with_open_delimiters_continues_onto_the_next_line() {
        let src = "[view]\ncol\n    canvas paint:(|rect| {\n        let a = 1;\n        draw(rect, a)\n    }) width:10\n";
        let doc = parse(src).expect("multi-line value parses");
        let ViewNode::Element(col) = &doc.view.nodes[0] else {
            panic!("expected the column");
        };
        let ViewNode::Element(canvas) = &col.children[0] else {
            panic!("expected the canvas");
        };
        let paint = canvas
            .attributes
            .iter()
            .find(|a| a.key == "paint")
            .expect("paint survived the join");
        assert!(
            paint.value.text().contains("let a = 1;")
                && paint.value.text().contains("draw(rect, a)"),
            "the whole closure is one value: {:?}",
            paint.value.text()
        );
        assert!(
            canvas.attributes.iter().any(|a| a.key == "width"),
            "and an attribute after the closing paren is still an attribute"
        );
    }
}
