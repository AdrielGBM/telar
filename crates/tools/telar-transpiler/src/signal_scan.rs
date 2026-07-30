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
