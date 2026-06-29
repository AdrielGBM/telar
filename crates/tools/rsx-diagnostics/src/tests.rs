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
        canvas_parameters: None,
        children: Vec::new(),
        line,
        content_start: 0,
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
fn render_points_at_the_offending_line() {
    let source = "[view]\ncolumn @card\n";
    let diag = Diagnostic::warning("nope", crate::Span::line(2));
    let rendered = diag.render(source);
    assert!(rendered.starts_with("warning: nope\n"));
    assert!(rendered.contains("--> line 2"));
    assert!(rendered.contains("2 | column @card"));
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
