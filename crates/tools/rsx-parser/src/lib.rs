//! Hand-written parser for `.rsx` source files (Proposal 4 syntax).
//!
//! A `.rsx` file has three sections:
//! - a leading **logic** zone of verbatim Rust (a `pub struct Props` here declares the component's props),
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
pub use lexer::{Section, header_section, is_preview_header};

/// Parses `.rsx` source text into an [`RsxDocument`].
pub fn parse(source: &str) -> Result<RsxDocument, ParseError> {
    parser::Parser::new(source).parse()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"[logic]
use rsx::prelude::*;
let count = signal(0i32);
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

@card
    width:    240
    padding:  20
    gap:      12
    direction: col
    align:    center

@badge: padding_x:6  padding_y:2  radius:6

[view]

col @card
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
            title.value, "C:\\x",
            "raw attr value keeps the backslash literal"
        );
        assert!(title.is_quoted);
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
                key: "size".into(),
                value: "14".into(),
                is_quoted: false,
                value_start: 0
            }
        );
        assert_eq!(
            text0.attributes[1],
            Attr {
                key: "color".into(),
                value: "dark".into(),
                is_quoted: false,
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
                value: "8".into(),
                is_quoted: false,
                value_start: 0
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
            inc.attributes[0],
            Attr {
                key: "fill".into(),
                value: "primary".into(),
                is_quoted: false,
                value_start: 0
            }
        );
        let on_press = inc.attributes.iter().find(|a| a.key == "on_press").unwrap();
        assert_eq!(on_press.value, "|| count.update(|n| *n += 1)");
    }

    #[test]
    fn parses_transition_attribute_to_end_of_line() {
        // Like closures, a `transition:` value runs verbatim to the end of the line so its spaces and comma-separated clauses survive tokenization.
        let doc = parse(
            "[view]\nbox fill:primary transition:opacity 200ms ease-out, fill 150ms linear\n",
        )
        .unwrap();
        let ViewNode::Element(b) = &doc.view.nodes[0] else {
            panic!("root should be an element");
        };
        let fill = b.attributes.iter().find(|a| a.key == "fill").unwrap();
        assert_eq!(fill.value, "primary");
        let transition = b.attributes.iter().find(|a| a.key == "transition").unwrap();
        assert_eq!(
            transition.value,
            "opacity 200ms ease-out, fill 150ms linear"
        );
    }

    #[test]
    fn transition_swallowing_trailing_attribute_errors() {
        // `transition:` runs to end of line, so a following `key:value` attribute would otherwise be
        // silently absorbed into the transition value instead of parsed as its own attribute.
        let err = parse("[view]\nbox transition:opacity 300ms align:center\n").unwrap_err();
        assert_eq!(err.line, 2);
        assert!(
            err.message
                .contains("transition: must be the last attribute on the line")
        );
    }

    #[test]
    fn transition_as_last_attribute_still_parses() {
        let doc = parse("[view]\nbox transition:opacity 300ms ease-in-out\n").unwrap();
        let ViewNode::Element(b) = &doc.view.nodes[0] else {
            panic!();
        };
        let transition = b.attributes.iter().find(|a| a.key == "transition").unwrap();
        assert_eq!(transition.value, "opacity 300ms ease-in-out");
    }

    #[test]
    fn transition_paren_form_still_parses() {
        let doc =
            parse("[view]\nbox transition(opacity 300ms ease-in-out) align:center\n").unwrap();
        let ViewNode::Element(b) = &doc.view.nodes[0] else {
            panic!();
        };
        let transition = b.attributes.iter().find(|a| a.key == "transition").unwrap();
        assert_eq!(transition.value, "opacity 300ms ease-in-out");
        let align = b.attributes.iter().find(|a| a.key == "align").unwrap();
        assert_eq!(align.value, "center");
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
                .any(|a| a.key == "ghost" && a.value.is_empty())
        );
        let on_press = reset
            .attributes
            .iter()
            .find(|a| a.key == "on_press")
            .unwrap();
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
    fn parses_canvas_with_params() {
        let src = "[logic]\n[view]\ncanvas @chart\n    |w, h|\n    rect\n";
        let doc = parse(src).unwrap();
        let ViewNode::Element(canvas) = &doc.view.nodes[0] else {
            panic!();
        };
        assert_eq!(canvas.tag, "canvas");
        assert_eq!(canvas.classes, vec!["chart".to_string()]);
        assert_eq!(canvas.leading_params.as_deref(), Some("w, h"));
        // The `|w, h|` line is consumed, leaving the rect as the only child.
        assert_eq!(canvas.children.len(), 1);
        let ViewNode::Element(rect) = &canvas.children[0] else {
            panic!();
        };
        assert_eq!(rect.tag, "rect");
    }

    #[test]
    fn leading_params_are_tag_agnostic() {
        // The `|…|` leading-child rule is generic: any element (not just `canvas`) captures it.
        let src = "[logic]\n[view]\nsurface @plot\n    |w, h|\n    rect\n";
        let doc = parse(src).unwrap();
        let ViewNode::Element(surface) = &doc.view.nodes[0] else {
            panic!();
        };
        assert_eq!(surface.tag, "surface");
        assert_eq!(surface.leading_params.as_deref(), Some("w, h"));
        assert_eq!(surface.children.len(), 1);
    }

    #[test]
    fn parses_named_closure_param_attribute() {
        let src = "[logic]\n[view]\nbtn \"x\" on_press:|ev| handle(ev)\n";
        let doc = parse(src).unwrap();
        let ViewNode::Element(btn) = &doc.view.nodes[0] else {
            panic!();
        };
        let on_press = btn.attributes.iter().find(|a| a.key == "on_press").unwrap();
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
    fn style_constant_without_value_errors() {
        let err = parse("[style]\nprimary:\n[view]\ncol\n").unwrap_err();
        assert_eq!(err.line, 2);
        assert!(err.message.contains("missing a value"));
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
    fn invalid_hex_constant_errors_but_valid_hex_parses() {
        for bad in ["#zzz", "#12", "#abcd", "#1234567"] {
            let src = format!("[style]\nc: {bad}\n[view]\ncol\n");
            let err = parse(&src).unwrap_err();
            assert!(
                err.message.contains("invalid hex"),
                "expected reject of {bad}"
            );
        }
        for good in ["#abc", "#3d78fa", "#3d78fa80"] {
            let src = format!("[style]\nc: {good}\n[view]\ncol\n");
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
}
