use std::collections::HashSet;

use telar_parser::{Element, RsxDocument, ViewNode};
use telar_transpiler::{color_attr_keys, color_keywords};

use crate::{Diagnostic, Span};

/// A filesystem-free view of the project's theme, used to validate color references against the
/// declared theme fields. Producers (e.g. the analyzer's `ProjectInfo`) build this so the crate
/// never has to touch disk.
pub struct ThemeView<'a> {
    pub theme_type: Option<&'a str>,
    pub theme_fields: &'a HashSet<String>,
}

/// A filesystem-free view of the project's baked i18n catalog, used to validate `t"key"` markup against the
/// keys the catalog actually defines. Built by the analyzer's `ProjectInfo` from `parse_catalog`, so this
/// crate never touches disk. `None` when the project has no catalog (i18n unused).
pub struct CatalogView<'a> {
    pub keys: &'a HashSet<String>,
}

/// Everything the per-node checks read, gathered once. Passed by reference through the walk so adding a
/// check does not mean threading another positional argument through every recursion site.
struct Ctx<'a> {
    defined_classes: HashSet<&'a str>,
    local_constants: HashSet<&'a str>,
    /// Every identifier token that appears in `[logic]`. A `widget "x"` splices the in-scope binding `x`, so a
    /// name absent from this set is a typo or a renamed binding. Membership (not a `let`-binding parse) keeps
    /// it conservative: destructured/patterned bindings still appear here, so a real binding is never a false
    /// positive; at worst a stray match suppresses a diagnostic (safe).
    logic_idents: HashSet<&'a str>,
    theme: Option<&'a ThemeView<'a>>,
    theme_configured: bool,
    catalog: Option<&'a CatalogView<'a>>,
}

/// Runs the `.rsx` semantic checks (undefined style classes, unknown color references, `widget` misuse,
/// unknown i18n keys) over a parsed document, returning neutral diagnostics. `theme` is `None` when the
/// project has no theme configured, `catalog` when it has no translations.
pub fn semantic_diagnostics(
    doc: &RsxDocument,
    theme: Option<&ThemeView>,
    catalog: Option<&CatalogView>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let ctx = Ctx {
        defined_classes: doc.style.classes.iter().map(|c| c.name.as_str()).collect(),
        local_constants: doc
            .style
            .constants
            .iter()
            .map(|c| c.name.as_str())
            .collect(),
        logic_idents: collect_idents(&doc.logic.source),
        theme,
        theme_configured: theme.map(|t| t.theme_type.is_some()).unwrap_or(false),
        catalog,
    };

    check_nodes(&doc.view.nodes, &ctx, false, &mut diagnostics);

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

fn check_nodes(nodes: &[ViewNode], ctx: &Ctx, reactive: bool, diagnostics: &mut Vec<Diagnostic>) {
    for node in nodes {
        match node {
            ViewNode::Element(el) => check_element(el, ctx, reactive, diagnostics),
            ViewNode::IfBlock(b) => {
                // The same test the transpiler makes: a `$signal` in the condition is what turns this into a rebuilding region.
                let reactive = reactive || b.condition.contains('$');
                check_nodes(&b.then_branch, ctx, reactive, diagnostics);
                if let Some(else_b) = &b.else_branch {
                    check_nodes(else_b, ctx, reactive, diagnostics);
                }
            }
            ViewNode::ForBlock(b) => {
                let reactive = reactive || b.iterable.trim_start().starts_with('$');
                check_nodes(&b.body, ctx, reactive, diagnostics);
            }
            ViewNode::LetStmt(_) => {}
        }
    }
}

fn check_element(el: &Element, ctx: &Ctx, reactive: bool, diagnostics: &mut Vec<Diagnostic>) {
    let span = Span::line(el.line);

    for class in &el.classes {
        if !ctx.defined_classes.contains(class.as_str()) {
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
        && !ctx.logic_idents.contains(name)
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

    check_i18n_keys(el, ctx, &span, diagnostics);
    check_theme_paths(el, ctx, &span, diagnostics);

    if ctx.theme_configured {
        let theme_fields = ctx.theme.map(|t| t.theme_fields);
        for attr in &el.attributes {
            if color_attr_keys().contains(&attr.key.as_str()) {
                let val = &attr.value;
                if val.starts_with('{')
                    || val.starts_with('#')
                    || val.starts_with('$')
                    || val.starts_with("Color::")
                    // An explicit `theme.field` is validated by `check_theme_paths`, which knows the dotted form.
                    || val.starts_with("theme.")
                    || color_keywords().contains(&val.as_str())
                {
                    continue;
                }
                let known = ctx.local_constants.contains(val.as_str())
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
    check_nodes(&el.children, ctx, reactive, diagnostics);
}

/// Flags an explicit `theme.field` reference — the spelling that reaches the theme from a non-color
/// attribute (`pad:theme.gutter`) — whose field the theme type does not declare.
///
/// The bare-ident form on a color attribute is already covered by the color check; this one applies to any
/// attribute, which is exactly why it needs its own pass.
fn check_theme_paths(el: &Element, ctx: &Ctx, span: &Span, diagnostics: &mut Vec<Diagnostic>) {
    let Some(theme) = ctx.theme.filter(|t| t.theme_type.is_some()) else {
        return;
    };
    for attr in &el.attributes {
        let Some(field) = attr.value.trim().strip_prefix("theme.") else {
            continue;
        };
        if field.is_empty() || !is_ident(field) || theme.theme_fields.contains(field) {
            continue;
        }
        diagnostics.push(Diagnostic::error(
            format!(
                "Unknown theme field `{field}` in `{}:theme.{field}`",
                attr.key
            ),
            span.clone(),
        ));
    }
}

/// Flags a `t"key"` — as element content (`text t"nav.title"`) or as an attribute value
/// (`btn label:t"buttons.save"`) — whose key the catalog does not define.
///
/// Worth catching here specifically: unlike the `t!` macro, which validates its key at compile time, a markup
/// key that misses falls back to rendering the key string itself. So the only symptom is `nav.titel` showing
/// up in the UI, with nothing failing anywhere.
fn check_i18n_keys(el: &Element, ctx: &Ctx, span: &Span, diagnostics: &mut Vec<Diagnostic>) {
    let Some(catalog) = ctx.catalog else {
        return;
    };
    let mut check = |key: &str, where_: &str| {
        let key = key.trim();
        if key.is_empty() || catalog.keys.contains(key) {
            return;
        }
        diagnostics.push(Diagnostic::warning(
            format!(
                "Unknown translation key `{key}` in {where_} — not defined in any locale catalog"
            ),
            span.clone(),
        ));
    };
    if el.content_i18n
        && let Some(content) = el.content.as_deref()
    {
        check(content, "t\"…\"");
    }
    for attr in &el.attributes {
        if attr.i18n {
            check(&attr.value, &format!("`{}:`", attr.key));
        }
    }
}
