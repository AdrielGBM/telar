use std::collections::HashSet;

use rsx_parser::{
    Attr, Element, ParseError, RsxDocument, StyleClass, StyleSection, ViewNode, ViewSection,
};

use crate::{Diagnostic, Severity, ThemeView, semantic_diagnostics};

fn element(classes: &[&str], attributes: Vec<Attr>, line: usize) -> Element {
    Element {
        tag: "column".into(),
        classes: classes.iter().map(|c| c.to_string()).collect(),
        attributes,
        content: None,
        leading_params: None,
        children: Vec::new(),
        line,
        content_start: 0,
        content_i18n: false,
    }
}

fn document(style: StyleSection, nodes: Vec<ViewNode>) -> RsxDocument {
    RsxDocument {
        logic: Default::default(),
        style,
        view: ViewSection { nodes },
        previews: Vec::new(),
    }
}

#[test]
fn parse_error_becomes_error_diagnostic_on_its_line() {
    let err = ParseError {
        message: "boom".into(),
        line: 7,
    };
    let diag = Diagnostic::from(err);
    assert_eq!(diag.severity, Severity::Error);
    assert_eq!(diag.span.line, 7);
    assert_eq!(diag.message, "boom");
}

#[test]
fn undefined_style_class_warns() {
    let style = StyleSection {
        constants: Vec::new(),
        classes: vec![StyleClass {
            name: "card".into(),
            props: Vec::new(),
            line: 2,
        }],
    };
    let doc = document(
        style,
        vec![ViewNode::Element(element(
            &["card", "missing"],
            Vec::new(),
            5,
        ))],
    );
    let diags = semantic_diagnostics(&doc, None);
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].severity, Severity::Warning);
    assert_eq!(diags[0].span.line, 5);
    assert!(diags[0].message.contains("@missing"));
}

#[test]
fn unknown_color_errors_only_when_theme_configured() {
    let attr = Attr {
        key: "color".into(),
        value: "nope".into(),
        is_quoted: false,
        i18n: false,
        value_start: 0,
    };
    let doc = document(
        StyleSection::default(),
        vec![ViewNode::Element(element(&[], vec![attr], 3))],
    );

    // No theme configured: the color check is skipped entirely.
    assert!(semantic_diagnostics(&doc, None).is_empty());

    // Theme configured but the color is neither a constant nor a theme field: error.
    let fields = HashSet::new();
    let theme = ThemeView {
        theme_type: Some("Theme"),
        theme_fields: &fields,
    };
    let diags = semantic_diagnostics(&doc, Some(&theme));
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].severity, Severity::Error);
    assert!(diags[0].message.contains("nope"));
}

// Builds a themed doc with one element carrying the given attributes, for the F49 color-value regression tests below.
fn themed_doc_with_attrs(attrs: Vec<(&str, &str)>) -> RsxDocument {
    let attributes = attrs
        .into_iter()
        .map(|(key, value)| Attr {
            key: key.into(),
            value: value.into(),
            is_quoted: false,
            i18n: false,
            value_start: 0,
        })
        .collect();
    document(
        StyleSection::default(),
        vec![ViewNode::Element(element(&[], attributes, 3))],
    )
}

#[test]
fn recognized_color_value_forms_produce_no_diagnostic_under_theme() {
    // `brand`/`accent` are declared as theme fields so the `from:`/`to:` gradient-stop cases resolve.
    let fields = HashSet::from(["brand".to_string(), "accent".to_string()]);
    let theme = ThemeView {
        theme_type: Some("Theme"),
        theme_fields: &fields,
    };

    for (key, value) in [
        ("fill", "white"),
        ("stroke", "$accent"),
        ("fill", "Color::RED"),
        ("from", "brand"),
        ("to", "accent"),
    ] {
        let doc = themed_doc_with_attrs(vec![(key, value)]);
        let diags = semantic_diagnostics(&doc, Some(&theme));
        assert!(
            diags.is_empty(),
            "expected no diagnostic for {key}:{value}, got {diags:?}"
        );
    }
}

#[test]
fn genuinely_unknown_color_still_errors_under_theme() {
    let fields = HashSet::new();
    let theme = ThemeView {
        theme_type: Some("Theme"),
        theme_fields: &fields,
    };
    let doc = themed_doc_with_attrs(vec![("fill", "bogus")]);
    let diags = semantic_diagnostics(&doc, Some(&theme));
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].severity, Severity::Error);
    assert!(diags[0].message.contains("bogus"));
}

#[cfg(feature = "lsp")]
#[test]
fn lsp_conversion_maps_severity_and_zero_based_line() {
    use lsp_types::{Diagnostic as LspDiagnostic, DiagnosticSeverity};

    let diag = Diagnostic::warning("x", crate::Span::line(1));
    let lsp: LspDiagnostic = (&diag).into();
    assert_eq!(lsp.severity, Some(DiagnosticSeverity::WARNING));
    // 1-based parser line 1 -> 0-based LSP line 0, spanning the whole line.
    assert_eq!(lsp.range.start.line, 0);
    assert_eq!(lsp.range.start.character, 0);
    assert_eq!(lsp.range.end.character, u32::MAX);
}

fn widget_element(name: &str, line: usize) -> Element {
    Element {
        tag: "widget".into(),
        classes: Vec::new(),
        attributes: Vec::new(),
        content: Some(name.to_string()),
        leading_params: None,
        children: Vec::new(),
        line,
        content_start: 0,
        content_i18n: false,
    }
}

fn document_with_logic(logic_src: &str, nodes: Vec<ViewNode>) -> RsxDocument {
    RsxDocument {
        logic: rsx_parser::LogicZone {
            source: logic_src.to_string(),
            ..Default::default()
        },
        style: StyleSection::default(),
        view: ViewSection { nodes },
        previews: Vec::new(),
    }
}

#[test]
fn widget_ref_to_defined_binding_is_ok() {
    let doc = document_with_logic(
        "let spring_box = Canvas::new(style, draw)?;",
        vec![ViewNode::Element(widget_element("spring_box", 4))],
    );
    assert!(semantic_diagnostics(&doc, None).is_empty());
}

fn if_block(condition: &str, body: Vec<ViewNode>) -> ViewNode {
    ViewNode::IfBlock(rsx_parser::IfBlock {
        condition: condition.to_string(),
        then_branch: body,
        else_branch: None,
        line: 3,
        condition_start: 0,
    })
}

#[test]
fn widget_inside_a_reactive_if_warns_before_the_build_does() {
    // The same rule the transpiler enforces as an E0507-shaped `compile_error!`, reported against the `.rsx` line instead.
    let doc = document_with_logic(
        "let icon = make_icon()?;\nlet shown = memo(move || true);",
        vec![if_block(
            "$shown",
            vec![ViewNode::Element(widget_element("icon", 4))],
        )],
    );
    let diags = semantic_diagnostics(&doc, None);
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].severity, Severity::Warning);
    assert_eq!(
        diags[0].span.line, 4,
        "reported at the widget, not the `if`"
    );
    assert!(diags[0].message.contains("reactive"));
    assert!(diags[0].message.contains("build"), "it names the fix");
}

#[test]
fn widget_inside_a_construction_time_if_is_fine() {
    // A plain condition picks its branch once, so splicing a binding there stays sound.
    let doc = document_with_logic(
        "let icon = make_icon()?;\nlet vertical = true;",
        vec![if_block(
            "vertical",
            vec![ViewNode::Element(widget_element("icon", 4))],
        )],
    );
    assert!(semantic_diagnostics(&doc, None).is_empty());
}

#[test]
fn widget_ref_to_unknown_binding_warns() {
    // The binding was renamed/typo'd, so the reference resolves to nothing in [logic].
    let doc = document_with_logic(
        "let spring_box = Canvas::new(style, draw)?;",
        vec![ViewNode::Element(widget_element("sprng_box", 4))],
    );
    let diags = semantic_diagnostics(&doc, None);
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].severity, Severity::Warning);
    assert_eq!(diags[0].span.line, 4);
    assert!(diags[0].message.contains("sprng_box"));
}
