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
    assert!(
        doc.logic.source.starts_with("use telar::prelude::*;"),
        "the logic zone is copied verbatim: {}",
        doc.logic.source
    );
    assert!(
        doc.logic.source.contains("fn reset() { count.set(0); }"),
        "a later line survived too: {}",
        doc.logic.source
    );
    assert!(
        doc.logic.source.contains("#[derive(Props)]"),
        "an attribute line survived: {}",
        doc.logic.source
    );
    // The section header must not leak into the logic zone.
    assert!(
        !doc.logic.source.contains("[style]"),
        "the section header leaked into the logic zone: {}",
        doc.logic.source
    );
}

#[test]
fn closure_attribute_requires_parenthesized_form() {
    // The colon form ran to end of line and swallowed the attributes after it, so it is rejected outright.
    let err = parse("[view]\nbtn \"x\" on_press:|| f() foo:bar\n").unwrap_err();
    assert!(err.message.contains("parenthesise it"), "{}", err.message);
    // The parenthesized form is delimited, so a trailing attribute can follow it on the same line.
    assert!(
        parse("[view]\nbtn \"x\" on_press:(|| f()) foo:bar\n").is_ok(),
        "a parenthesised closure is delimited, so an attribute may follow it"
    );
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
    // `key(…)` is the one form admitting a space at depth 0, so a multi-clause spec survives tokenization and the attributes either side of it still parse.
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
    // A computed colon value balances parens, so `fill:chip_fill($snap, id)` is read whole and a following attribute on the same line still parses.
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

/// `transition:` was the one key whose colon value ran to end of line, which is also why it needed a bespoke check for the attributes that run swallowed. It obeys the one rule now, so nothing is swallowed and nothing has to be last.
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
    assert!(
        reset
            .attributes
            .iter()
            .any(|a| a.key == "ghost" && a.value.is_flag()),
        "a bare word is a flag attribute: {:?}",
        reset.attributes
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

/// `else if` used to match the plain-`else` check and have the rest of its line thrown away, silently — the branch ran unconditionally and nothing said so. It chains now, and the absence of this test is why the fault survived.
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
    let src =
        "[logic]\n[view]\ncol\n    for item in $items key item.id gap:8\n        text \"{item}\"\n";
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
    assert!(
        doc.logic.source.is_empty(),
        "an empty document has no logic"
    );
    assert!(doc.style.classes.is_empty(), "nor any style class");
    assert!(doc.view.nodes.is_empty(), "nor any view node");
}

#[test]
fn document_without_logic_zone() {
    let src = "[logic]\n[view]\ntext \"hello\"\n";
    let doc = parse(src).unwrap();
    assert!(
        doc.logic.source.is_empty(),
        "an empty [logic] zone parses to no source"
    );
    assert_eq!(doc.view.nodes.len(), 1);
}

#[test]
fn inconsistent_indentation_errors_instead_of_dropping_nodes() {
    let src = "[view]\nbox\n     text \"a\"\n    text \"b\"\n    text \"c\"\n";
    let err = parse(src).unwrap_err();
    assert_eq!(err.line, 4);
    assert!(
        err.message.contains("indentation"),
        "the error must name the cause: {}",
        err.message
    );
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
    assert!(
        err.message.contains("invalid hex"),
        "the error must name the cause: {}",
        err.message
    );
    assert!(
        parse("[view]\nbox shadow_color:#00000040\n").is_ok(),
        "eight hex digits are a colour with alpha"
    );
    assert!(
        parse("[view]\ntext label:\"#hashtag\"\n").is_ok(),
        "a # inside a string is text, not a colour"
    );
}

#[test]
fn style_class_prop_empty_or_bad_hex_errors() {
    let err = parse("[style]\n@card\n    width:\n[view]\ncol\n").unwrap_err();
    assert_eq!(err.line, 3);
    assert!(
        err.message.contains("missing a value"),
        "the error must name the cause: {}",
        err.message
    );
    let err = parse("[style]\n@card: bg:#zz\n[view]\ncol\n").unwrap_err();
    assert_eq!(err.line, 2);
    assert!(
        err.message.contains("invalid hex"),
        "the error must name the cause: {}",
        err.message
    );
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
    // `#abcd` is the four-digit form `Color::from_hex` has always accepted at runtime; rejecting it here made a legal colour a parse error.
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
    assert_eq!(doc.view.nodes.len(), 1);
    assert_eq!(doc.previews.len(), 2);

    assert_eq!(doc.previews[0].name, "Default");
    assert!(
        doc.previews[0].options.is_empty(),
        "an unqualified preview carries no options"
    );
    let ViewNode::Element(comp) = &doc.previews[0].body[0] else {
        panic!("preview body should be a view element");
    };
    assert_eq!(comp.tag, "counter");

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
/// A value whose delimiters are still open at end of line continues onto the next, so a closure can be written where it is used instead of being bound in `[logic]` and referred to by name. Without this a `canvas` could never carry its own drawing, and the only way to place one was the `widget` escape.
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
        paint.value.text().contains("let a = 1;") && paint.value.text().contains("draw(rect, a)"),
        "the whole closure is one value: {:?}",
        paint.value.text()
    );
    assert!(
        canvas.attributes.iter().any(|a| a.key == "width"),
        "and an attribute after the closing paren is still an attribute"
    );
}

const PREVIEWS_EXAMPLE: &str = r#"[previews "Forms/Checkbox" layout:centered matrix:themes]
Use `toggle` for a setting that applies at once, a checkbox for one confirmed with a button.

[preview "Default"]
checkbox label:"I agree to the terms"

[preview "Bound" args(agree:false)]
col gap:8
    checkbox checked:$agree label:"I agree"
    text "agree · {$agree}"

[play]
canvas.click(by_role(Role::CheckBox).named("I agree"))?;
canvas.expect_text("agree · true")?;
"#;

fn parse_err(src: &str) -> ParseError {
    parse(src).expect_err("the document should be refused")
}

#[test]
fn parses_the_previews_meta_args_and_play() {
    let doc = parse(PREVIEWS_EXAMPLE).unwrap();
    assert!(
        doc.view.nodes.is_empty(),
        "a previews-only file has no view"
    );

    let meta = doc.previews_meta.expect("the meta section");
    assert_eq!(meta.title.as_deref(), Some("Forms/Checkbox"));
    assert_eq!(
        meta.options,
        vec![
            StyleProp {
                key: "layout".into(),
                value: "centered".into(),
            },
            StyleProp {
                key: "matrix".into(),
                value: "themes".into(),
            },
        ]
    );
    assert_eq!(
        meta.body,
        "Use `toggle` for a setting that applies at once, a checkbox for one confirmed with a button."
    );
    assert_eq!(meta.line, 1);

    assert_eq!(doc.previews.len(), 2);
    let default = &doc.previews[0];
    assert!(default.args.is_empty() && default.play.is_none());

    let bound = &doc.previews[1];
    assert_eq!(bound.name, "Bound");
    assert!(bound.options.is_empty(), "`args(…)` is not an option");
    assert_eq!(bound.args.len(), 1);
    assert_eq!(bound.args[0].name, "agree");
    assert_eq!(bound.args[0].default, Value::Expr("false".into()));
    assert!(PREVIEWS_EXAMPLE[bound.args[0].default_start..].starts_with("false)]"));
    let ViewNode::Element(col) = &bound.body[0] else {
        panic!("the variant's body is view markup");
    };
    assert_eq!(col.children.len(), 2);

    let play = bound.play.as_ref().expect("the play zone");
    assert_eq!(play.line, 12);
    assert_eq!(play.start_line, 13);
    assert_eq!(
        play.source,
        "canvas.click(by_role(Role::CheckBox).named(\"I agree\"))?;\ncanvas.expect_text(\"agree · true\")?;"
    );
}

#[test]
fn an_args_default_reads_like_an_attribute_value() {
    let src = "[view]\ncol\n\n[preview \"Labels\" args(label:\"I agree\" count:(1 + 2) tint:#3d78fa) args:none]\nbutton label:$label\n";
    let doc = parse(src).unwrap();
    let preview = &doc.previews[0];
    let defaults: Vec<(&str, &Value)> = preview
        .args
        .iter()
        .map(|arg| (arg.name.as_str(), &arg.default))
        .collect();
    assert_eq!(
        defaults,
        vec![
            ("label", &Value::Quoted("I agree".into())),
            ("count", &Value::Expr("(1 + 2)".into())),
            ("tint", &Value::Expr("#3d78fa".into())),
        ]
    );
    assert!(src[preview.args[0].default_start..].starts_with("I agree\""));
    assert!(src[preview.args[1].default_start..].starts_with("(1 + 2)"));
    assert_eq!(
        preview.options,
        vec![StyleProp {
            key: "args".into(),
            value: "none".into(),
        }],
        "the `args:none` opt-out stays an option"
    );
}

#[test]
fn a_header_option_keeps_the_spaces_inside_its_brackets() {
    let src = "[previews matrix:(mode:[light dark] locale:[en ar]) group:\"A B\"]\n";
    let meta = parse(src).unwrap().previews_meta.unwrap();
    assert_eq!(meta.title, None, "the title is optional");
    assert_eq!(
        meta.options,
        vec![
            StyleProp {
                key: "matrix".into(),
                value: "(mode:[light dark] locale:[en ar])".into(),
            },
            StyleProp {
                key: "group".into(),
                value: "\"A B\"".into(),
            },
        ]
    );
    assert!(meta.body.is_empty());
}

#[test]
fn previews_is_a_header_of_its_own() {
    assert!(is_previews_header("[previews \"Forms/Checkbox\"]"));
    assert!(is_previews_header("[previews]"));
    assert!(!is_preview_header("[previews \"Forms/Checkbox\"]"));
    assert!(!is_previews_header("[preview \"Default\"]"));
    assert!(!is_previews_header("[previewsish]"));
}

#[test]
fn a_previews_file_holds_previews_without_a_view() {
    let src = "[logic]\nuse crate::checkbox::checkbox;\n\n[preview \"Default\"]\ncheckbox\n";
    let doc = parse(src).unwrap();
    assert!(doc.view.nodes.is_empty());
    assert!(doc.previews_meta.is_none());
    assert_eq!(doc.previews.len(), 1);
    assert_eq!(doc.logic.source, "use crate::checkbox::checkbox;");
}

#[test]
fn an_empty_play_zone_is_kept() {
    let doc = parse("[preview \"A\"]\nbox\n[play]\n\n[preview \"B\"]\nbox\n").unwrap();
    let play = doc.previews[0]
        .play
        .as_ref()
        .expect("an empty [play] is still there");
    assert!(play.source.is_empty());
    assert_eq!(play.start_line, 0);
    assert!(doc.previews[1].play.is_none());
}

#[test]
fn misplaced_preview_sections_are_refused() {
    let cases = [
        ("[view]\ncol\n[play]\nlet x = 1;\n", 3, "[play]"),
        (
            "[preview \"A\"]\nbox\n[play]\na();\n[play]\nb();\n",
            5,
            "[play]",
        ),
        ("[preview \"A\"]\nbox\n[previews \"T\"]\n", 3, "[previews]"),
        ("[previews \"T\"]\n[previews \"U\"]\n", 2, "[previews]"),
    ];
    for (src, line, names) in cases {
        let error = parse_err(src);
        assert_eq!(error.line, line, "{src:?}: {}", error.message);
        assert!(error.message.contains(names), "{src:?}: {}", error.message);
    }
}

#[test]
fn malformed_preview_headers_are_refused() {
    let cases = [
        ("[previews \"T\" args(a:1)]\n", "belongs on a `[preview]`"),
        ("[preview \"A\" args(agree)]\nbox\n", "`name:default`"),
        ("[preview \"A\" args(agree:)]\nbox\n", "needs a default"),
        ("[preview \"A\" args(a:1 a:2)]\nbox\n", "twice"),
        (
            "[preview \"A\" args(a:1) args(b:2)]\nbox\n",
            "one `args(…)`",
        ),
        ("[preview \"A\" args(a:(1]\nbox\n", "unterminated"),
        ("[preview \"A\" width(360)]\nbox\n", "`width:(…)`"),
        ("[preview Default]\nbox\n", "quoted name"),
        ("[preview width:360 \"A\"]\nbox\n", "quoted name"),
    ];
    for (src, needle) in cases {
        let error = parse_err(src);
        assert!(error.message.contains(needle), "{src:?}: {}", error.message);
    }
}

#[test]
fn a_line_knows_its_preview_section() {
    let section = |line| find_section_at(PREVIEWS_EXAMPLE, line);
    assert_eq!(section(0), Section::Previews);
    assert_eq!(section(1), Section::Prose);
    assert_eq!(section(3), Section::Preview);
    assert_eq!(section(4), Section::View);
    assert_eq!(section(8), Section::View);
    assert_eq!(section(11), Section::Play);
    assert_eq!(section(12), Section::Play);

    let after_play = "[preview \"A\"]\nbox\n[play]\na();\n[preview \"B\"]\nbox\n";
    assert_eq!(
        find_section_at(after_play, 5),
        Section::View,
        "a preview after a play zone is markup again"
    );
    assert_eq!(section_opened_by("[preview \"B\"]"), Some(Section::View));
    assert_eq!(section_opened_by("[previews]"), Some(Section::Prose));
    assert_eq!(section_opened_by("[play]"), Some(Section::Play));
    assert_eq!(section_opened_by("box"), None);
}

#[test]
fn an_args_default_may_be_a_braced_struct_literal() {
    let src = "[view]\ncol\n\n[preview \"A\" args(x:Foo{ a: 1 } y:(Bar { b: 2 }) n:2)]\nbox\n";
    let preview = &parse(src).unwrap().previews[0];
    assert_eq!(preview.args.len(), 3);
    assert_eq!(preview.args[0].default, Value::Expr("Foo{ a: 1 }".into()));
    assert_eq!(
        preview.args[1].default,
        Value::Expr("(Bar { b: 2 })".into())
    );
    assert_eq!(preview.args[2].default, Value::Expr("2".into()));
}

#[test]
fn a_header_option_may_hold_braces() {
    let src = "[view]\ncol\n\n[preview \"A\" matrix:{ a b } dark]\nbox\n";
    let preview = &parse(src).unwrap().previews[0];
    let options: Vec<(&str, &str)> = preview
        .options
        .iter()
        .map(|o| (o.key.as_str(), o.value.as_str()))
        .collect();
    assert_eq!(options, vec![("matrix", "{ a b }"), ("dark", "")]);
}

#[test]
fn a_view_attribute_value_with_braces_stays_one_token() {
    let src = "[view]\nbox on_press:(|| f(Foo { a: 1 })) gap:8\n";
    let doc = parse(src).unwrap();
    let ViewNode::Element(el) = &doc.view.nodes[0] else {
        panic!("an element");
    };
    assert_eq!(el.attributes.len(), 2);
}
