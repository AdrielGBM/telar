//! The checks past parsing: unknown attributes, unreachable branches and missing catalogue keys.

use std::collections::HashSet;

use telar_parser::{Element, RsxDocument, ViewNode};

use crate::{Diagnostic, Span};

/// A filesystem-free view of the project's baked i18n catalog, used to validate `t"key"` markup against the keys the catalog actually defines. Built by the analyzer's `ProjectInfo` from `parse_catalog`, so this crate never touches disk. `None` when the project has no catalog (i18n unused).
pub struct CatalogView<'a> {
    pub keys: &'a HashSet<String>,
}

/// Everything the per-node checks read, gathered once. Passed by reference through the walk so adding a check does not mean threading another positional argument through every recursion site.
struct Ctx<'a> {
    defined_classes: HashSet<&'a str>,
    catalog: Option<&'a CatalogView<'a>>,
}

/// Runs the `.rsx` semantic checks (undefined style classes, unquoted captures, unknown i18n keys) over a parsed document, returning neutral diagnostics. `catalog` is `None` when the project has no translations.
///
/// A *value* is not checked here at all: it is a Rust expression, and rustc judges it against the `.rsx` line the source map points at. The checks that remain are the ones about the markup's own vocabulary — a class nobody declared, an i18n key the catalogue does not hold.
pub fn semantic_diagnostics(doc: &RsxDocument, catalog: Option<&CatalogView>) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let ctx = Ctx {
        defined_classes: doc.style.classes.iter().map(|c| c.name.as_str()).collect(),
        catalog,
    };

    check_nodes(&doc.view.nodes, &ctx, false, &mut diagnostics);
    diagnostics.extend(unsigiled_captures(doc));

    diagnostics
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

    check_i18n_keys(el, ctx, &span, diagnostics);

    // Reactivity is inherited: a plain `row` nested inside a reactive branch is rebuilt with it.
    check_nodes(&el.children, ctx, reactive, diagnostics);
}

/// Flags a `t"key"` — as element content (`text t"nav.title"`) or as an attribute value (`btn label:t"buttons.save"`) — whose key the catalog does not define.
///
/// Worth catching here specifically: unlike the `t!` macro, which validates its key at compile time, a markup key that misses falls back to rendering the key string itself. So the only symptom is `nav.titel` showing up in the UI, with nothing failing anywhere.
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
    // An attribute's catalogue lookup is `t!("…")` now, and the macro validates its own key.
}

/// Bindings a `[logic]` zone constructs from something that is certainly not `Copy`, with the line each was bound on.
///
/// Certainly, not probably: only shapes whose type is knowable from the text alone. Nothing here has type information, so a guess would warn about an `i32` a closure captures perfectly well, and a warning that fires on correct code is worse than none.
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

/// Every closure body the view contains, as raw text. Enough to ask which of them name a binding, which is all this check needs and all it can get without types.
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
/// The second closure to capture it finds it moved. rustc says so eventually and says it well — the terminal maps the error onto this line — but it says it after a compile, and its advice is to clone, which is the bookkeeping the sigil exists to remove. `$name` is the answer, and this is early enough to be the first place the author reads it.
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

    /// Nothing here knows types, so the check only fires on constructions whose type the text settles. A `Copy` binding captured by two closures is correct code, and warning about it would be the worse error.
    #[test]
    fn a_binding_that_might_be_copy_is_left_alone() {
        let found = warnings(
            "[logic]\nlet count = compute();\n\n[view]\ncol\n    button label:\"a\" on_press:(|| { use_it(count); })\n    button label:\"b\" on_press:(|| { use_it(count); })\n",
        );
        assert!(found.is_empty(), "{found:?}");
    }
}
