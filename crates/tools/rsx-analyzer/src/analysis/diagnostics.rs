use crate::position::parser_line_to_lsp_range;
use crate::project::ProjectInfo;
use rsx_parser::{Element, RsxDocument, ViewNode};
use std::collections::HashSet;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

pub fn semantic_diagnostics(doc: &RsxDocument, project: Option<&ProjectInfo>) -> Vec<Diagnostic> {
    let mut diags = Vec::new();

    let defined_classes: HashSet<&str> =
        doc.style.classes.iter().map(|c| c.name.as_str()).collect();
    let local_constants: HashSet<&str> = doc
        .style
        .constants
        .iter()
        .map(|c| c.name.as_str())
        .collect();

    let theme_configured = project.map(|p| p.theme_type.is_some()).unwrap_or(false);

    check_nodes(
        &doc.view.nodes,
        &defined_classes,
        &local_constants,
        project,
        theme_configured,
        &mut diags,
    );

    diags
}

fn check_nodes(
    nodes: &[ViewNode],
    defined_classes: &HashSet<&str>,
    local_constants: &HashSet<&str>,
    project: Option<&ProjectInfo>,
    theme_configured: bool,
    diags: &mut Vec<Diagnostic>,
) {
    for node in nodes {
        match node {
            ViewNode::Element(el) => check_element(
                el,
                defined_classes,
                local_constants,
                project,
                theme_configured,
                diags,
            ),
            ViewNode::IfBlock(b) => {
                check_nodes(
                    &b.then_branch,
                    defined_classes,
                    local_constants,
                    project,
                    theme_configured,
                    diags,
                );
                if let Some(else_b) = &b.else_branch {
                    check_nodes(
                        else_b,
                        defined_classes,
                        local_constants,
                        project,
                        theme_configured,
                        diags,
                    );
                }
            }
            ViewNode::ForBlock(b) => {
                check_nodes(
                    &b.body,
                    defined_classes,
                    local_constants,
                    project,
                    theme_configured,
                    diags,
                );
            }
            ViewNode::LetStmt { .. } => {}
        }
    }
}

fn check_element(
    el: &Element,
    defined_classes: &HashSet<&str>,
    local_constants: &HashSet<&str>,
    project: Option<&ProjectInfo>,
    theme_configured: bool,
    diags: &mut Vec<Diagnostic>,
) {
    let range = parser_line_to_lsp_range(el.line);

    for class in &el.classes {
        if !defined_classes.contains(class.as_str()) {
            diags.push(Diagnostic {
                range,
                severity: Some(DiagnosticSeverity::WARNING),
                message: format!("Style class `.{class}` is not defined in [style]"),
                ..Default::default()
            });
        }
    }

    if theme_configured {
        let theme_fields = project.map(|p| &p.theme_fields);
        for attr in &el.attrs {
            if matches!(attr.key.as_str(), "color" | "fill" | "stroke" | "outline") {
                let val = &attr.value;
                if val.starts_with('{') || val.starts_with('#') {
                    continue;
                }
                let known = local_constants.contains(val.as_str())
                    || theme_fields
                        .map(|f| f.contains(val.as_str()))
                        .unwrap_or(false);
                if !known {
                    diags.push(Diagnostic {
                        range,
                        severity: Some(DiagnosticSeverity::ERROR),
                        message: format!(
                            "Unknown color `{val}` — not in [style] constants or theme fields"
                        ),
                        ..Default::default()
                    });
                }
            }
        }
    }

    for child in &el.children {
        check_nodes(
            std::slice::from_ref(child),
            defined_classes,
            local_constants,
            project,
            theme_configured,
            diags,
        );
    }
}
