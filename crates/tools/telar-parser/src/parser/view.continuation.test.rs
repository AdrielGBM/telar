use crate::{Element, ParseError, ViewNode, continued_element_line, parse};

fn view(body: &str) -> Vec<ViewNode> {
    parse(&format!("[view]\n{body}"))
        .unwrap_or_else(|error| panic!("line {}: {}", error.line, error.message))
        .view
        .nodes
}

fn refused(body: &str) -> ParseError {
    parse(&format!("[view]\n{body}")).expect_err("the view should be refused")
}

fn element(node: &ViewNode) -> &Element {
    let ViewNode::Element(element) = node else {
        panic!("expected an element, got {node:?}");
    };
    element
}

fn assert_same_header(continued: &Element, one_line: &Element) {
    assert_eq!(continued.tag, one_line.tag);
    assert_eq!(continued.classes, one_line.classes);
    assert_eq!(continued.attributes, one_line.attributes);
    assert_eq!(continued.content, one_line.content);
}

#[test]
fn attributes_split_over_lines_read_as_the_one_line_header() {
    for (continued, one_line) in [
        (
            "row role:button toggled:$selected\n        on_press:(move || flip(selected, on_toggle.as_ref()))\n        align:center gap:(shared::gap_sm(size))\n",
            "row role:button toggled:$selected on_press:(move || flip(selected, on_toggle.as_ref())) align:center gap:(shared::gap_sm(size))\n",
        ),
        (
            "box @card \"hi\"\n    fill:accent\n    hover_style(fill:$theme.accent_hover) transition(fill 150ms ease-out)\n    disabled\n",
            "box @card \"hi\" fill:accent hover_style(fill:$theme.accent_hover) transition(fill 150ms ease-out)\n    disabled\n",
        ),
        (
            "Button label:\"Save\"\n      ghost:false wide\n",
            "Button label:\"Save\" ghost:false wide\n",
        ),
        (
            "row\n        gap:4\n\n        align:center\n",
            "row gap:4 align:center\n",
        ),
    ] {
        let continued = view(continued);
        let one_line = view(one_line);
        assert_same_header(element(&continued[0]), element(&one_line[0]));
    }
}

#[test]
fn a_directive_continues_the_header_like_any_attribute() {
    let nodes = view(
        "box fill:accent\n        hover_style(fill:$theme.accent_hover)\n        transition(fill 150ms ease-out)\n",
    );
    let keys: Vec<&str> = element(&nodes[0])
        .attributes
        .iter()
        .map(|attr| attr.key.as_str())
        .collect();
    assert_eq!(keys, ["fill", "hover_style", "transition"]);
}

#[test]
fn children_follow_the_continuation_lines_at_their_own_indent() {
    for body in [
        "row gap:4\n        align:center\n    glyph size:2\n    text \"label\"\n",
        "row gap:4\n    align:center\n    glyph size:2\n    text \"label\"\n",
    ] {
        let nodes = view(body);
        let row = element(&nodes[0]);
        assert_eq!(row.attributes.len(), 2, "{body}");
        let tags: Vec<&str> = row
            .children
            .iter()
            .map(|c| element(c).tag.as_str())
            .collect();
        assert_eq!(tags, ["glyph", "text"], "{body}");
    }
}

#[test]
fn a_child_may_continue_its_own_header() {
    let nodes = view("col\n    text \"a\"\n        size:12\n        bold\n    text \"b\"\n");
    let col = element(&nodes[0]);
    assert!(col.attributes.is_empty());
    let first = element(&col.children[0]);
    let keys: Vec<&str> = first.attributes.iter().map(|a| a.key.as_str()).collect();
    assert_eq!(
        keys,
        ["size"],
        "a bare flag opening a line is a child of the text"
    );
    assert_eq!(element(&first.children[0]).tag, "bold");
    assert_eq!(element(&col.children[1]).content.as_deref(), Some("b"));
}

#[test]
fn continuation_lines_work_inside_if_for_and_match_bodies() {
    let nodes = view(
        "if $open\n    box fill:accent\n            gap:4\nelse\n    box\n        gap:2\nfor item in $items\n    text \"{item}\"\n        size:12\nmatch $state\n    Ready(v)\n        text \"{v}\"\n            bold:true\n",
    );
    let ViewNode::IfBlock(block) = &nodes[0] else {
        panic!("expected the if");
    };
    assert_eq!(element(&block.then_branch[0]).attributes.len(), 2);
    let else_branch = block.else_branch.as_ref().expect("the else");
    assert_eq!(element(&else_branch[0]).attributes[0].key, "gap");
    let ViewNode::ForBlock(for_block) = &nodes[1] else {
        panic!("expected the for");
    };
    assert_eq!(element(&for_block.body[0]).attributes[0].key, "size");
    let ViewNode::MatchBlock(match_block) = &nodes[2] else {
        panic!("expected the match");
    };
    assert_eq!(
        element(&match_block.arms[0].body[0]).attributes[0].key,
        "bold"
    );
}

#[test]
fn a_preview_body_continues_its_component_call() {
    let doc = parse("[preview \"Default\"]\nCheckbox label:\"I agree\"\n        checked:true\n")
        .expect("the preview parses");
    let call = element(&doc.previews[0].body[0]);
    let keys: Vec<&str> = call.attributes.iter().map(|a| a.key.as_str()).collect();
    assert_eq!(keys, ["label", "checked"]);
}

#[test]
fn an_attribute_on_a_continuation_line_points_at_its_own_bytes() {
    let source =
        "[view]\nrow gap:4\n        on_press:(move || flip(selected))\n        label:\"Ok\"\n";
    let doc = parse(source).expect("parses");
    let row = element(&doc.view.nodes[0]);
    for attr in &row.attributes {
        assert!(
            source[attr.value_start..].starts_with(attr.value.text()),
            "`{}` starts at its own text: {:?}",
            attr.key,
            &source[attr.value_start..]
        );
    }
    let on_press = &row.attributes[1];
    assert_eq!(
        source[..on_press.value_start].lines().count(),
        3,
        "`on_press` is on the third line"
    );
    assert_eq!(row.line, 2, "the element's line is its header's first");
}

#[test]
fn content_on_a_continuation_line_points_at_its_own_bytes() {
    let source = "[view]\ntext size:12\n    color:muted \"hello {name}\"\n";
    let doc = parse(source).expect("parses");
    let text = element(&doc.view.nodes[0]);
    assert!(source[text.content_start..].starts_with("hello {name}"));
}

#[test]
fn an_error_on_a_continuation_line_is_reported_on_that_line() {
    for (body, line, why) in [
        ("box gap:4\n        gap:8\n", 3, "given twice"),
        ("box gap:4\n        fill:#12\n", 3, "hex"),
        (
            "box\n    gap:4\n    fill:#fff label:\"a\" \"b\" \"c\"\n",
            4,
            "content string",
        ),
    ] {
        let error = refused(body);
        assert_eq!(error.line, line, "{body}: {}", error.message);
        assert!(
            error.message.to_lowercase().contains(why),
            "{body}: {}",
            error.message
        );
    }
}

#[test]
fn an_attribute_line_no_element_can_take_is_refused() {
    for (body, line) in [
        ("gap:4\n", 2),
        ("if $open\n    gap:4\n", 3),
        ("for x in xs\n    gap:4\n", 3),
        ("col\n    text \"a\"\n    gap:4\n", 4),
        ("row gap:4\n        disabled\n        align:center\n", 4),
        ("match $state\n    Ready(v)\n        gap:4\n", 4),
    ] {
        let error = refused(body);
        assert_eq!(error.line, line, "{body}: {}", error.message);
        assert!(
            error
                .message
                .contains("continues an element's attribute list"),
            "{body}: {}",
            error.message
        );
    }
}

#[test]
fn a_value_left_open_still_runs_on_inside_a_continued_header() {
    let nodes = view(
        "canvas width:10\n        paint:(|rect| {\n            draw(rect)\n        })\n        height:20\n",
    );
    let canvas = element(&nodes[0]);
    let keys: Vec<&str> = canvas.attributes.iter().map(|a| a.key.as_str()).collect();
    assert_eq!(keys, ["width", "paint", "height"]);
    assert!(canvas.attributes[1].value.text().contains("draw(rect)"));
    assert!(canvas.children.is_empty());
}

#[test]
fn tooling_finds_the_header_a_continuation_line_belongs_to() {
    let source = "[view]\ncol\n    row gap:4\n            on_press:(move || {\n                go()\n            })\n            align:center\n        text \"a\"\n";
    assert_eq!(continued_element_line(source, 3), Some(2));
    assert_eq!(continued_element_line(source, 6), Some(2));
    assert_eq!(
        continued_element_line(source, 4),
        None,
        "a line inside an open value"
    );
    assert_eq!(continued_element_line(source, 2), None, "the header itself");
    assert_eq!(continued_element_line(source, 7), None, "a child");
}
