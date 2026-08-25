use std::collections::HashSet;

use telar_parser::{
    Attr, Element, ParseError, RsxDocument, StyleClass, StyleSection, Value, ViewNode, ViewSection,
};

use crate::{CatalogView, Diagnostic, Severity, ThemeView, semantic_diagnostics};

fn element(classes: &[&str], attributes: Vec<Attr>, line: usize) -> Element {
    Element {
        tag: "column".into(),
        classes: classes.iter().map(|c| c.to_string()).collect(),
        attributes,
        content: None,
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
    let diags = semantic_diagnostics(&doc, None, None);
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].severity, Severity::Warning);
    assert_eq!(diags[0].span.line, 5);
    assert!(diags[0].message.contains("@missing"));
}

#[test]
fn unknown_color_errors_only_when_theme_configured() {
    let attr = Attr {
        key: "color".into(),
        value: Value::Bare("nope".into()),
        value_start: 0,
    };
    let doc = document(
        StyleSection::default(),
        vec![ViewNode::Element(element(&[], vec![attr], 3))],
    );

    // No theme configured: the color check is skipped entirely.
    assert!(semantic_diagnostics(&doc, None, None).is_empty());

    // Theme configured but the color is neither a constant nor a theme field: error.
    let fields = HashSet::new();
    let theme = ThemeView {
        theme_type: Some("Theme"),
        theme_fields: &fields,
    };
    let diags = semantic_diagnostics(&doc, Some(&theme), None);
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
            value: Value::Bare(value.into()),
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
        let diags = semantic_diagnostics(&doc, Some(&theme), None);
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
    let diags = semantic_diagnostics(&doc, Some(&theme), None);
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].severity, Severity::Error);
    assert!(diags[0].message.contains("bogus"));
}

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
        children: Vec::new(),
        line,
        content_start: 0,
        content_i18n: false,
    }
}

fn document_with_logic(logic_src: &str, nodes: Vec<ViewNode>) -> RsxDocument {
    RsxDocument {
        logic: telar_parser::LogicZone {
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
    assert!(semantic_diagnostics(&doc, None, None).is_empty());
}

fn if_block(condition: &str, body: Vec<ViewNode>) -> ViewNode {
    ViewNode::IfBlock(telar_parser::IfBlock {
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
    let diags = semantic_diagnostics(&doc, None, None);
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
    assert!(semantic_diagnostics(&doc, None, None).is_empty());
}

#[test]
fn widget_ref_to_unknown_binding_warns() {
    // The binding was renamed/typo'd, so the reference resolves to nothing in [logic].
    let doc = document_with_logic(
        "let spring_box = Canvas::new(style, draw)?;",
        vec![ViewNode::Element(widget_element("sprng_box", 4))],
    );
    let diags = semantic_diagnostics(&doc, None, None);
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].severity, Severity::Warning);
    assert_eq!(diags[0].span.line, 4);
    assert!(diags[0].message.contains("sprng_box"));
}

fn i18n_element(key: &str, line: usize) -> Element {
    Element {
        tag: "text".into(),
        classes: Vec::new(),
        attributes: Vec::new(),
        content: Some(key.to_string()),
        children: Vec::new(),
        line,
        content_start: 0,
        content_i18n: true,
    }
}

fn catalog_keys() -> HashSet<String> {
    HashSet::from(["nav.title".to_string(), "buttons.save".to_string()])
}

#[test]
fn unknown_i18n_key_in_markup_warns() {
    // A missing markup key renders the key string itself at runtime, so nothing else would ever report it.
    let doc = document(
        StyleSection::default(),
        vec![ViewNode::Element(i18n_element("nav.titel", 6))],
    );
    let keys = catalog_keys();
    let catalog = CatalogView { keys: &keys };
    let diags = semantic_diagnostics(&doc, None, Some(&catalog));
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].severity, Severity::Warning);
    assert_eq!(diags[0].span.line, 6);
    assert!(diags[0].message.contains("nav.titel"));
}

#[test]
fn known_i18n_key_is_ok_in_content_and_in_an_attribute() {
    let attr = Attr {
        key: "label".into(),
        value: Value::I18n("buttons.save".into()),
        value_start: 0,
    };
    let mut el = i18n_element("nav.title", 3);
    el.attributes = vec![attr];
    let doc = document(StyleSection::default(), vec![ViewNode::Element(el)]);
    let keys = catalog_keys();
    let catalog = CatalogView { keys: &keys };
    assert!(semantic_diagnostics(&doc, None, Some(&catalog)).is_empty());
}

#[test]
fn unknown_i18n_key_in_an_attribute_warns_and_names_the_attribute() {
    let attr = Attr {
        key: "label".into(),
        value: Value::I18n("buttons.sav".into()),
        value_start: 0,
    };
    let doc = document(
        StyleSection::default(),
        vec![ViewNode::Element(element(&[], vec![attr], 4))],
    );
    let keys = catalog_keys();
    let catalog = CatalogView { keys: &keys };
    let diags = semantic_diagnostics(&doc, None, Some(&catalog));
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert!(diags[0].message.contains("buttons.sav"));
    assert!(diags[0].message.contains("label"), "it names where");
}

#[test]
fn a_project_without_a_catalog_gets_no_key_diagnostics() {
    // Otherwise every `t"…"` in a project that has not added translations yet would light up.
    let doc = document(
        StyleSection::default(),
        vec![ViewNode::Element(i18n_element("anything.at.all", 2))],
    );
    assert!(semantic_diagnostics(&doc, None, None).is_empty());
}

#[test]
fn a_plain_quoted_string_is_not_checked_as_a_key() {
    // `label:"Save"` is a literal, not a lookup; only the `t"…"` form is a `Value::I18n`.
    let attr = Attr {
        key: "label".into(),
        value: Value::Quoted("Save".into()),
        value_start: 0,
    };
    let doc = document(
        StyleSection::default(),
        vec![ViewNode::Element(element(&[], vec![attr], 4))],
    );
    let keys = catalog_keys();
    let catalog = CatalogView { keys: &keys };
    assert!(semantic_diagnostics(&doc, None, Some(&catalog)).is_empty());
}

#[test]
fn unknown_theme_path_errors_in_a_non_color_attribute() {
    // `pad:` never reaches the colour check, so without this pass a typo'd token is silent until rustc.
    let attr = Attr {
        key: "pad".into(),
        value: Value::Bare("theme.guttr".into()),
        value_start: 0,
    };
    let fields = HashSet::from(["gutter".to_string()]);
    let theme = ThemeView {
        theme_type: Some("Theme"),
        theme_fields: &fields,
    };
    let doc = document(
        StyleSection::default(),
        vec![ViewNode::Element(element(&[], vec![attr], 7))],
    );
    let diags = semantic_diagnostics(&doc, Some(&theme), None);
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].severity, Severity::Error);
    assert_eq!(diags[0].span.line, 7);
    assert!(diags[0].message.contains("guttr"));
    assert!(diags[0].message.contains("pad"), "it names the attribute");
}

#[test]
fn known_theme_path_is_ok_and_a_bare_ident_is_not_treated_as_one() {
    let fields = HashSet::from(["gutter".to_string()]);
    let theme = ThemeView {
        theme_type: Some("Theme"),
        theme_fields: &fields,
    };
    for value in ["theme.gutter", "card_gap", "12"] {
        let attr = Attr {
            key: "pad".into(),
            value: Value::Bare(value.into()),
            value_start: 0,
        };
        let doc = document(
            StyleSection::default(),
            vec![ViewNode::Element(element(&[], vec![attr], 3))],
        );
        let diags = semantic_diagnostics(&doc, Some(&theme), None);
        assert!(diags.is_empty(), "{value} should be fine, got {diags:?}");
    }
}

#[test]
fn a_theme_path_on_a_colour_attribute_is_checked_once_as_a_path() {
    // Both passes see `fill:theme.x`; only the path one may speak, and a typo must still be caught exactly once.
    let fields = HashSet::from(["primary".to_string()]);
    let theme = ThemeView {
        theme_type: Some("Theme"),
        theme_fields: &fields,
    };
    let ok = themed_doc_with_attrs(vec![("fill", "theme.primary")]);
    assert!(
        semantic_diagnostics(&ok, Some(&theme), None).is_empty(),
        "a valid theme path is not an unknown colour"
    );

    let bad = themed_doc_with_attrs(vec![("fill", "theme.primry")]);
    let diags = semantic_diagnostics(&bad, Some(&theme), None);
    assert_eq!(
        diags.len(),
        1,
        "reported once, not by both passes: {diags:?}"
    );
    assert!(diags[0].message.contains("primry"));
}
