use std::collections::HashSet;

use rsx_parser::{Element, RsxDocument, ViewNode};

use crate::{Diagnostic, Span};

/// A filesystem-free view of the project's theme, used to validate color references against the
/// declared theme fields. Producers (e.g. the analyzer's `ProjectInfo`) build this so the crate
/// never has to touch disk.
pub struct ThemeView<'a> {
    pub theme_type: Option<&'a str>,
    pub theme_fields: &'a HashSet<String>,
}

/// Runs the `.rsx` semantic checks (undefined style classes, unknown color references) over a parsed
/// document, returning neutral diagnostics. `theme` is `None` when the project has no theme configured.
pub fn semantic_diagnostics(doc: &RsxDocument, theme: Option<&ThemeView>) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    let defined_classes: HashSet<&str> =
        doc.style.classes.iter().map(|c| c.name.as_str()).collect();
    let local_constants: HashSet<&str> = doc
        .style
        .constants
        .iter()
        .map(|c| c.name.as_str())
        .collect();

    let theme_configured = theme.map(|t| t.theme_type.is_some()).unwrap_or(false);

    check_nodes(
        &doc.view.nodes,
        &defined_classes,
        &local_constants,
        theme,
        theme_configured,
        &mut diagnostics,
    );

    diagnostics
}

fn check_nodes(
    nodes: &[ViewNode],
    defined_classes: &HashSet<&str>,
    local_constants: &HashSet<&str>,
    theme: Option<&ThemeView>,
    theme_configured: bool,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for node in nodes {
        match node {
            ViewNode::Element(el) => check_element(
                el,
                defined_classes,
                local_constants,
                theme,
                theme_configured,
                diagnostics,
            ),
            ViewNode::IfBlock(b) => {
                check_nodes(
                    &b.then_branch,
                    defined_classes,
                    local_constants,
                    theme,
                    theme_configured,
                    diagnostics,
                );
                if let Some(else_b) = &b.else_branch {
                    check_nodes(
                        else_b,
                        defined_classes,
                        local_constants,
                        theme,
                        theme_configured,
                        diagnostics,
                    );
                }
            }
            ViewNode::ForBlock(b) => {
                check_nodes(
                    &b.body,
                    defined_classes,
                    local_constants,
                    theme,
                    theme_configured,
                    diagnostics,
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
    theme: Option<&ThemeView>,
    theme_configured: bool,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let span = Span::line(el.line);

    for class in &el.classes {
        if !defined_classes.contains(class.as_str()) {
            diagnostics.push(Diagnostic::warning(
                format!("Style class `.{class}` is not defined in [style]"),
                span.clone(),
            ));
        }
    }

    if theme_configured {
        let theme_fields = theme.map(|t| t.theme_fields);
        for attr in &el.attributes {
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
                    diagnostics.push(Diagnostic::error(
                        format!("Unknown color `{val}` — not in [style] constants or theme fields"),
                        span.clone(),
                    ));
                }
            }
        }
    }

    check_nodes(
        &el.children,
        defined_classes,
        local_constants,
        theme,
        theme_configured,
        diagnostics,
    );
}
