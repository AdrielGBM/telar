//! Detects reactive signals declared in the logic zone so the view generator knows which identifiers must be read with `.get()` inside closures.

use crate::naming::is_ident;

#[derive(Debug, Clone)]
pub struct SignalInfo {
    pub name: String,
    // All signal kinds currently read via `.get()`; kind is retained to drive future kind-specific codegen.
    #[allow(dead_code)]
    pub kind: SignalKind,
    // 0-based index of the declaring line within `logic_source.lines()`, so callers can test "declared above line j" without re-parsing.
    pub line_index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalKind {
    RwSignal,
    Memo,
}

/// Every identifier the logic zone binds with a top-level `let`, so a bare name in the view resolves to the
/// binding the author wrote three lines above instead of an ambient theme token that happens to share its
/// spelling. Without this the shadowing runs the wrong way: `let size = props.size` is unreachable from
/// `font_size:size`, and a binding named after a real token (`radius`, `spacing`, `muted`) silently reads the theme.
///
/// Only bindings at the zone's own indentation count — anything deeper belongs to a nested `fn` or block and is
/// not in scope where the view is emitted. Destructuring patterns contribute every name they bind.
pub fn scan_locals(logic_source: &str) -> Vec<String> {
    let base = logic_source
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .unwrap_or(0);

    let mut locals = Vec::new();
    for raw in logic_source.lines() {
        if raw.trim().is_empty() || raw.len() - raw.trim_start().len() != base {
            continue;
        }
        let Some(rest) = raw.trim_start().strip_prefix("let ") else {
            continue;
        };
        let pattern = rest.split('=').next().unwrap_or("").trim_end_matches(';');
        for name in pattern_bindings(pattern) {
            if !locals.contains(&name) {
                locals.push(name);
            }
        }
    }
    locals
}

/// The identifiers a `let` pattern binds. A simple `name: Type` keeps only the name; anything with a
/// destructuring delimiter yields every identifier in it, since telling a bound name from a path segment there
/// needs a real parser and over-collecting only costs a shadowed token.
fn pattern_bindings(pattern: &str) -> Vec<String> {
    let destructures = pattern.contains(['(', '{', '[', ',']);
    let pattern = if destructures {
        pattern
    } else {
        pattern.split(':').next().unwrap_or("")
    };

    let mut names = Vec::new();
    for word in pattern.split(|c: char| !c.is_alphanumeric() && c != '_') {
        if matches!(word, "mut" | "ref" | "let" | "else" | "") || !is_ident(word) {
            continue;
        }
        if word.starts_with(|c: char| c.is_ascii_uppercase()) {
            continue;
        }
        names.push(word.to_string());
    }
    names
}

/// Scans the logic source for signal declarations and returns their names.
///
/// Recognised forms:
/// - `let NAME = signal(...)`
/// - `let NAME = memo(...)`
pub fn scan_signals(logic_source: &str) -> Vec<SignalInfo> {
    let mut signals = Vec::new();

    for (line_index, raw) in logic_source.lines().enumerate() {
        let line = raw.trim();
        let Some(rest) = line.strip_prefix("let ") else {
            continue;
        };

        let Some((binding, expr)) = rest.split_once('=') else {
            continue;
        };
        let binding = binding.trim();
        let expr = expr.trim_start();

        let kind = if expr.starts_with("signal(") {
            SignalKind::RwSignal
        } else if expr.starts_with("memo(") {
            SignalKind::Memo
        } else {
            continue;
        };

        // Simple binding: strip an optional `mut` and a type annotation.
        let name = binding
            .strip_prefix("mut ")
            .unwrap_or(binding)
            .split(':')
            .next()
            .unwrap_or("")
            .trim();
        if is_ident(name) {
            signals.push(SignalInfo {
                name: name.to_string(),
                kind,
                line_index,
            });
        }
    }

    signals
}

/// Every identifier the logic zone binds to an `effect(…)`.
///
/// An `Effect` deregisters on drop, so one bound to a `let` here would stop the moment the component
/// function returns — running exactly once and never again, with nothing to see in the source. The view
/// generator hands these to the root widget so they live as long as the tree they belong to. An `effect(…)`
/// that is never bound at all is already a loud `must_use` warning and needs nothing from this.
/// Whether `expr` opens with a call to `effect`, however it is spelled — bare, `telar::effect`, or through any
/// other path. The bare form was the only one recognised, so `let e = telar::effect(…)` — the spelling an
/// application reaches for when it is not inside a `use telar::*` — was silently not kept alive.
fn opens_an_effect(expr: &str) -> bool {
    let expr = expr.trim_start();
    let Some(head) = expr.split('(').next() else {
        return false;
    };
    head.rsplit("::").next().map(str::trim) == Some("effect")
}

/// The `[logic]` bindings that hold an `Effect`, so the view can keep them alive past the function that made
/// them. A handle that drops deregisters its effect, which runs once and then stops.
pub fn scan_effects(logic_source: &str) -> Vec<String> {
    let mut effects = Vec::new();
    for raw in logic_source.lines() {
        let Some(rest) = raw.trim().strip_prefix("let ") else {
            continue;
        };
        let Some((binding, expr)) = rest.split_once('=') else {
            continue;
        };
        if !opens_an_effect(expr) {
            continue;
        }
        let name = binding
            .trim()
            .strip_prefix("mut ")
            .unwrap_or(binding.trim())
            .split(':')
            .next()
            .unwrap_or("")
            .trim();
        if is_ident(name) && !effects.contains(&name.to_string()) {
            effects.push(name.to_string());
        }
    }
    effects
}

/// Rewrites a `let NAME = signal(EXPR)` logic line into the keyed hot-reload form
/// `let NAME = telar::hot_signal_auto!("<fn_name>::<NAME>", EXPR)` so `cargo telar dev` can snapshot
/// and restore the value across dylib swaps. Returns `None` when the line is not a signal binding
/// (memos are derived state and recompute from their sources, so they are left untouched).
pub fn hot_rewrite_signal_decl(line: &str, fn_name: &str) -> Option<String> {
    let indent_len = line.len() - line.trim_start().len();
    let (indent, trimmed) = line.split_at(indent_len);
    let rest = trimmed.strip_prefix("let ")?;
    let (binding, expr) = rest.split_once('=')?;
    let expr_trimmed = expr.trim_start();
    let args = expr_trimmed.strip_prefix("signal(")?;
    let name = binding
        .trim()
        .strip_prefix("mut ")
        .unwrap_or(binding.trim())
        .split(':')
        .next()
        .unwrap_or("")
        .trim();
    if !is_ident(name) {
        return None;
    }
    Some(format!(
        "{indent}let {} = telar::hot_signal_auto!(\"{fn_name}::{name}\", {args}",
        binding.trim()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hot_rewrite_keys_signal_binding() {
        let out = hot_rewrite_signal_decl("let count = signal(0i32);", "counter").unwrap();
        assert_eq!(
            out,
            "let count = telar::hot_signal_auto!(\"counter::count\", 0i32);"
        );
    }

    #[test]
    fn hot_rewrite_skips_memos_and_plain_lets() {
        assert!(hot_rewrite_signal_decl("let d = memo(move || 1);", "c").is_none());
        assert!(hot_rewrite_signal_decl("let x = 5;", "c").is_none());
        assert!(hot_rewrite_signal_decl("count.set(signal_like);", "c").is_none());
    }

    #[test]
    fn hot_rewrite_preserves_nested_parens_and_mut() {
        let out = hot_rewrite_signal_decl("let mut v = signal(vec![(1, 2)]);", "grid").unwrap();
        assert_eq!(
            out,
            "let mut v = telar::hot_signal_auto!(\"grid::v\", vec![(1, 2)]);"
        );
    }

    #[test]
    fn detects_rw_signal() {
        let s = scan_signals("let count = signal(0i32);");
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].name, "count");
        assert_eq!(s[0].kind, SignalKind::RwSignal);
    }

    #[test]
    fn detects_memo() {
        let s = scan_signals("let double = memo(move |_| count.get() * 2);");
        assert_eq!(s[0].name, "double");
        assert_eq!(s[0].kind, SignalKind::Memo);
    }

    #[test]
    fn ignores_plain_let() {
        let s = scan_signals("let x = 5;");
        assert!(s.is_empty());
    }
}

#[cfg(test)]
mod effect_scan_tests {
    use super::*;

    /// The spelling the scanner used to miss, and the one every application outside a `use telar::*` writes.
    #[test]
    fn a_path_qualified_effect_is_recognised() {
        assert_eq!(scan_effects("let e = telar::effect(|| {});"), vec!["e"]);
        assert_eq!(scan_effects("let e = crate::effect(|| {});"), vec!["e"]);
        assert_eq!(scan_effects("let e = effect(|| {});"), vec!["e"]);
    }

    #[test]
    fn a_binding_that_is_not_an_effect_is_left_alone() {
        assert!(scan_effects("let s = signal(0);").is_empty());
        assert!(scan_effects("let m = memo(|| 1);").is_empty());
        // A name merely *ending* in `effect` is a different function.
        assert!(scan_effects("let e = side_effect(|| {});").is_empty());
    }
}
