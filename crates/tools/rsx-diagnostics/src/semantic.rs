use std::collections::HashSet;

use rsx_parser::{Element, RsxDocument, ViewNode};
use rsx_transpiler::{color_attr_keys, color_keywords};

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
    // Every identifier token that appears in `[logic]`. A `widget "x"` splices the in-scope binding `x`,
    // so a name absent from this set is a typo or a renamed binding — flagged below. Membership (not a
    // `let`-binding parse) keeps it conservative: destructured/patterned bindings still appear here, so a
    // real binding is never a false positive; at worst a stray match suppresses a diagnostic (safe).
    let logic_idents = collect_idents(&doc.logic.source);

    let theme_configured = theme.map(|t| t.theme_type.is_some()).unwrap_or(false);

    check_nodes(
        &doc.view.nodes,
        &defined_classes,
        &local_constants,
        &logic_idents,
        theme,
        theme_configured,
        false,
        &mut diagnostics,
    );

    diagnostics
}

/// Collects every identifier-shaped token in `src` (start `_`/letter, rest `_`/alphanumeric).
fn collect_idents(src: &str) -> HashSet<&str> {
    let bytes = src.as_bytes();
    let mut set = HashSet::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'_' || bytes[i].is_ascii_alphabetic() {
            let start = i;
            while i < bytes.len() && (bytes[i] == b'_' || bytes[i].is_ascii_alphanumeric()) {
                i += 1;
            }
            set.insert(&src[start..i]);
        } else {
            i += 1;
        }
    }
    set
}

/// Whether `s` is a valid Rust identifier.
fn is_ident(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c == '_' || c.is_ascii_alphabetic())
        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

#[allow(clippy::too_many_arguments)]
fn check_nodes(
    nodes: &[ViewNode],
    defined_classes: &HashSet<&str>,
    local_constants: &HashSet<&str>,
    logic_idents: &HashSet<&str>,
    theme: Option<&ThemeView>,
    theme_configured: bool,
    reactive: bool,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for node in nodes {
        match node {
            ViewNode::Element(el) => check_element(
                el,
                defined_classes,
                local_constants,
                logic_idents,
                theme,
                theme_configured,
                reactive,
                diagnostics,
            ),
            ViewNode::IfBlock(b) => {
                // The same test the transpiler makes: a `$signal` in the condition is what turns this into a rebuilding region.
                let reactive = reactive || b.condition.contains('$');
                check_nodes(
                    &b.then_branch,
                    defined_classes,
                    local_constants,
                    logic_idents,
                    theme,
                    theme_configured,
                    reactive,
                    diagnostics,
                );
                if let Some(else_b) = &b.else_branch {
                    check_nodes(
                        else_b,
                        defined_classes,
                        local_constants,
                        logic_idents,
                        theme,
                        theme_configured,
                        reactive,
                        diagnostics,
                    );
                }
            }
            ViewNode::ForBlock(b) => {
                let reactive = reactive || b.iterable.trim_start().starts_with('$');
                check_nodes(
                    &b.body,
                    defined_classes,
                    local_constants,
                    logic_idents,
                    theme,
                    theme_configured,
                    reactive,
                    diagnostics,
                );
            }
            ViewNode::LetStmt(_) => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn check_element(
    el: &Element,
    defined_classes: &HashSet<&str>,
    local_constants: &HashSet<&str>,
    logic_idents: &HashSet<&str>,
    theme: Option<&ThemeView>,
    theme_configured: bool,
    reactive: bool,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let span = Span::line(el.line);

    for class in &el.classes {
        if !defined_classes.contains(class.as_str()) {
            diagnostics.push(Diagnostic::warning(
                format!("Style class `@{class}` is not defined in [style]"),
                span.clone(),
            ));
        }
    }

    // `widget "x"` splices the in-scope `[logic]` binding `x`; flag a reference to a name that appears
    // nowhere in [logic] (a typo or a binding renamed out from under it) at edit time, before rustc reports
    // it against the generated code. Only for identifier-shaped names — a non-identifier is already a hard
    // transpile error (`compile_error!`), so no diagnostic is needed here.
    if el.tag == "widget"
        && let Some(name) = el.content.as_deref().map(str::trim)
        && !name.is_empty()
        && is_ident(name)
        && !logic_idents.contains(name)
    {
        diagnostics.push(Diagnostic::warning(
            format!("`widget \"{name}\"` references `{name}`, which is not defined in [logic]"),
            span.clone(),
        ));
    }

    // The transpiler already turns this into a `compile_error!`; repeating it here is what puts the squiggle in the editor rather than at the next build.
    if el.tag == "widget"
        && reactive
        && let Some(name) = el.content.as_deref().map(str::trim)
        && !name.is_empty()
    {
        diagnostics.push(Diagnostic::warning(
            format!(
                "`widget \"{name}\"` cannot be used inside a reactive `if`/`for`, which rebuilds its \
                 content. Use `build \"{name}()?\"` — an expression evaluated per build."
            ),
            span.clone(),
        ));
    }

    if theme_configured {
        let theme_fields = theme.map(|t| t.theme_fields);
        for attr in &el.attributes {
            if color_attr_keys().contains(&attr.key.as_str()) {
                let val = &attr.value;
                if val.starts_with('{')
                    || val.starts_with('#')
                    || val.starts_with('$')
                    || val.starts_with("Color::")
                    || color_keywords().contains(&val.as_str())
                {
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

    // Reactivity is inherited: a plain `row` nested inside a reactive branch is rebuilt with it.
    check_nodes(
        &el.children,
        defined_classes,
        local_constants,
        logic_idents,
        theme,
        theme_configured,
        reactive,
        diagnostics,
    );
}
