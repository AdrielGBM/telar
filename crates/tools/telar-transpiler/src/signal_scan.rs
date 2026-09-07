//! Detects reactive signals declared in the logic zone so the view generator knows which identifiers must be read with `.get()` inside closures.

use telar_project::naming::is_ident;

#[derive(Debug, Clone)]
/// A signal declared in `[logic]`: its name, its kind and the line it was declared on.
pub struct SignalInfo {
    pub name: String,
    // Retained to drive future kind-specific codegen; every kind currently reads via `.get()`.
    #[allow(dead_code)]
    pub kind: SignalKind,
    // 0-based within `logic_source.lines()`, so "declared above line j" is a comparison, not a re-parse.
    pub line_index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Which reactive primitive a `[logic]` binding is.
pub enum SignalKind {
    RwSignal,
    Memo,
}

/// Every identifier the logic zone binds with a top-level `let`, so a bare name in the view resolves to the binding the author wrote three lines above instead of an ambient theme token that happens to share its spelling. Without this the shadowing runs the wrong way: `let size = props.size` is unreachable from `font_size:size`, and a binding named after a real token (`radius`, `spacing`, `muted`) silently reads the theme.
///
/// Only bindings at the zone's own indentation count — anything deeper belongs to a nested `fn` or block and is not in scope where the view is emitted. Destructuring patterns contribute every name they bind.
pub fn scan_locals(logic_source: &str) -> Vec<String> {
    let base = logic_source
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .unwrap_or(0);

    // A binding of the generated scope, not of `[logic]`, and one a rebuilding closure must clone rather than move — `#[derive(Props)]` writes the `Clone` that allows it.
    let mut locals = vec!["props".to_string()];
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

/// The identifiers a `let` pattern binds. A simple `name: Type` keeps only the name; anything with a destructuring delimiter yields every identifier in it, since telling a bound name from a path segment there needs a real parser and over-collecting only costs a shadowed token.
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
/// An `Effect` deregisters on drop, so one bound to a `let` here would stop the moment the component function returns — running exactly once and never again, with nothing to see in the source. The view generator hands these to the root widget so they live as long as the tree they belong to. An `effect(…)` that is never bound at all is already a loud `must_use` warning and needs nothing from this. Whether `expr` opens with a call to `effect`, however it is spelled — bare, `telar::effect`, or through any other path. The bare form was the only one recognised, so `let e = telar::effect(…)` — the spelling an application reaches for when it is not inside a `use telar::*` — was silently not kept alive.
fn opens_an_effect(expr: &str) -> bool {
    let expr = expr.trim_start();
    let Some(head) = expr.split('(').next() else {
        return false;
    };
    head.rsplit("::").next().map(str::trim) == Some("effect")
}

/// The `[logic]` bindings that hold an `Effect`, so the view can keep them alive past the function that made them. A handle that drops deregisters its effect, which runs once and then stops.
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

/// Rewrites a `let NAME = signal(EXPR)` logic line into the keyed hot-reload form `let NAME = telar::hot_signal_auto!("<fn_name>::<NAME>", EXPR)` so `cargo telar dev` can snapshot and restore the value across dylib swaps. Returns `None` when the line is not a signal binding (memos are derived state and recompute from their sources, so they are left untouched).
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
#[path = "signal_scan_test.rs"]
mod tests;

#[cfg(test)]
#[path = "signal_scan_effect_scan_test.rs"]
mod effect_scan_tests;
