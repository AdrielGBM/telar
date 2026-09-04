//! Guards over the semantic checks and the LSP conversion.

use std::collections::HashSet;

use telar_parser::{
    Attr, Element, ParseError, RsxDocument, StyleClass, StyleSection, ViewNode, ViewSection,
};

use crate::{CatalogView, Diagnostic, Severity, semantic_diagnostics};

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
fn lsp_conversion_maps_severity_and_zero_based_line() {
    use lsp_types::{Diagnostic as LspDiagnostic, DiagnosticSeverity};

    let diag = Diagnostic::warning("x", crate::Span::line(1));
    let lsp: LspDiagnostic = (&diag).into();
    assert_eq!(lsp.severity, Some(DiagnosticSeverity::WARNING));
    assert_eq!(lsp.range.start.line, 0);
    assert_eq!(lsp.range.start.character, 0);
    assert_eq!(lsp.range.end.character, u32::MAX);
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
    let diags = semantic_diagnostics(&doc, Some(&catalog));
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].severity, Severity::Warning);
    assert_eq!(diags[0].span.line, 6);
    assert!(diags[0].message.contains("nav.titel"));
}

#[test]
fn a_project_without_a_catalog_gets_no_key_diagnostics() {
    // Otherwise every `t"…"` in a project that has not added translations yet would light up.
    let doc = document(
        StyleSection::default(),
        vec![ViewNode::Element(i18n_element("anything.at.all", 2))],
    );
    assert!(semantic_diagnostics(&doc, None).is_empty());
}
