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
    diagnostics.extend(unsigiled_captures(doc));

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
            ViewNode::MatchBlock(b) => {
                let reactive = reactive || b.scrutinee.contains('$');
                for arm in &b.arms {
                    check_nodes(&arm.body, ctx, reactive, diagnostics);
                }
            }
            ViewNode::LetStmt(_) | ViewNode::Comment(_) => {}
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
                let val = attr.value.text();
                if val.starts_with('{')
                    || val.starts_with('#')
                    || val.starts_with('$')
                    || val.starts_with("Color::")
                    // An explicit `theme.field` is validated by `check_theme_paths`, which knows the dotted form.
                    || val.starts_with("theme.")
                    || color_keywords().contains(&val)
                {
                    continue;
                }
                let known = ctx.local_constants.contains(val)
                    || theme_fields.map(|f| f.contains(val)).unwrap_or(false);
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
        let Some(field) = attr.value.text().trim().strip_prefix("theme.") else {
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
        if attr.value.is_i18n() {
            check(attr.value.text(), &format!("`{}:`", attr.key));
        }
    }
}

/// Bindings a `[logic]` zone constructs from something that is certainly not `Copy`, with the line each was
/// bound on.
///
/// Certainly, not probably: only shapes whose type is knowable from the text alone. Nothing here has type
/// information, so a guess would warn about an `i32` a closure captures perfectly well, and a warning that
/// fires on correct code is worse than none.
fn non_copy_bindings(logic: &str) -> Vec<(String, usize)> {
    const CONSTRUCTORS: [&str; 8] = [
        "Rc::new(",
        "Arc::new(",
        "String::from(",
        "Vec::new(",
        "vec![",
        "HashMap::new(",
        "BTreeMap::new(",
        ".to_string()",
    ];
    logic
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let rest = line.trim().strip_prefix("let ")?;
            if !CONSTRUCTORS.iter().any(|c| rest.contains(c)) {
                return None;
            }
            let name = rest
                .split(['=', ':'])
                .next()?
                .trim()
                .trim_start_matches("mut ")
                .trim();
            is_plain_ident(name).then(|| (name.to_string(), index + 1))
        })
        .collect()
}

fn is_plain_ident(s: &str) -> bool {
    !s.is_empty()
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !s.chars().next().is_some_and(|c| c.is_ascii_digit())
}

/// Every closure body the view contains, as raw text. Enough to ask which of them name a binding, which is
/// all this check needs and all it can get without types.
fn closure_bodies(nodes: &[ViewNode], out: &mut Vec<String>) {
    for node in nodes {
        match node {
            ViewNode::Element(el) => {
                for attr in &el.attributes {
                    let text = attr.value.text();
                    if attr.key.starts_with("on_") || text.contains("||") {
                        out.push(text.to_string());
                    }
                }
                closure_bodies(&el.children, out);
            }
            ViewNode::ForBlock(block) => closure_bodies(&block.body, out),
            ViewNode::IfBlock(block) => {
                closure_bodies(&block.then_branch, out);
                if let Some(other) = &block.else_branch {
                    closure_bodies(other, out);
                }
            }
            ViewNode::MatchBlock(block) => {
                for arm in &block.arms {
                    closure_bodies(&arm.body, out);
                }
            }
            _ => {}
        }
    }
}

/// Warns when a binding that cannot be `Copy` is captured, without its sigil, by more than one closure.
///
/// The second closure to capture it finds it moved. rustc says so eventually and says it well — the terminal
/// maps the error onto this line — but it says it after a compile, and its advice is to clone, which is the
/// bookkeeping the sigil exists to remove. `$name` is the answer, and this is early enough to be the first
/// place the author reads it.
fn unsigiled_captures(doc: &RsxDocument) -> Vec<Diagnostic> {
    let mut bodies = Vec::new();
    closure_bodies(&doc.view.nodes, &mut bodies);
    non_copy_bindings(&doc.logic.source)
        .into_iter()
        .filter(|(name, _)| {
            let sigiled = format!("${name}");
            bodies
                .iter()
                .filter(|body| mentions(body, name) && !body.contains(&sigiled))
                .count()
                > 1
        })
        .map(|(name, line)| {
            Diagnostic::warning(
                format!(
                    "`{name}` is captured by more than one closure without its sigil; write `${name}` so each gets its own copy"
                ),
                Span::line(line),
            )
        })
        .collect()
}

/// Whether `body` names `binding` as a whole word, so `held` does not match `withheld`.
fn mentions(body: &str, binding: &str) -> bool {
    body.match_indices(binding).any(|(at, _)| {
        let before = body[..at].chars().next_back();
        let after = body[at + binding.len()..].chars().next();
        let boundary = |c: Option<char>| !c.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_');
        boundary(before) && boundary(after)
    })
}

#[cfg(test)]
mod capture_tests {
    use super::*;

    fn warnings(src: &str) -> Vec<String> {
        let doc = telar_parser::parse(src).expect("the probe parses");
        unsigiled_captures(&doc)
            .into_iter()
            .map(|d| d.message)
            .collect()
    }

    /// The case the compiler catches a whole build later, and answers with the wrong advice for this language.
    #[test]
    fn a_non_copy_binding_captured_twice_without_its_sigil_is_reported() {
        let found = warnings(
            "[logic]\nlet held = Rc::new(RefCell::new(0));\n\n[view]\ncol\n    button label:\"a\" on_press:(|| { held.take(); })\n    button label:\"b\" on_press:(|| { held.take(); })\n",
        );
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].contains("`$held`"), "{}", found[0]);
    }

    /// Sigiled is the fix, so saying it again would be noise.
    #[test]
    fn the_sigil_settles_it() {
        let found = warnings(
            "[logic]\nlet held = Rc::new(RefCell::new(0));\n\n[view]\ncol\n    button label:\"a\" on_press:(|| { $held.take(); })\n    button label:\"b\" on_press:(|| { $held.take(); })\n",
        );
        assert!(found.is_empty(), "{found:?}");
    }

    /// One closure moves it and that is correct, so there is nothing to warn about.
    #[test]
    fn one_closure_may_take_it() {
        let found = warnings(
            "[logic]\nlet held = Rc::new(RefCell::new(0));\n\n[view]\ncol\n    button label:\"a\" on_press:(|| { held.take(); })\n",
        );
        assert!(found.is_empty(), "{found:?}");
    }

    /// Nothing here knows types, so the check only fires on constructions whose type the text settles. A
    /// `Copy` binding captured by two closures is correct code, and warning about it would be the worse error.
    #[test]
    fn a_binding_that_might_be_copy_is_left_alone() {
        let found = warnings(
            "[logic]\nlet count = compute();\n\n[view]\ncol\n    button label:\"a\" on_press:(|| { use_it(count); })\n    button label:\"b\" on_press:(|| { use_it(count); })\n",
        );
        assert!(found.is_empty(), "{found:?}");
    }
}
