//! Detects reactive signals declared in the logic zone so the view generator
//! knows which identifiers must be read with `.get()` inside closures.

use crate::naming::is_ident;

#[derive(Debug, Clone)]
pub struct SignalInfo {
    pub name: String,
    // All signal kinds currently read via `.get()`; kind is retained to drive future kind-specific codegen.
    #[allow(dead_code)]
    pub kind: SignalKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalKind {
    RwSignal,
    Memo,
}

/// Scans the logic source for signal declarations and returns their names.
///
/// Recognised forms:
/// - `let NAME = create_rw_signal(...)`
/// - `let NAME = create_memo(...)`
pub fn scan_signals(logic_source: &str) -> Vec<SignalInfo> {
    let mut signals = Vec::new();

    for raw in logic_source.lines() {
        let line = raw.trim();
        let Some(rest) = line.strip_prefix("let ") else {
            continue;
        };

        let Some((binding, expr)) = rest.split_once('=') else {
            continue;
        };
        let binding = binding.trim();
        let expr = expr.trim_start();

        let kind = if expr.starts_with("create_rw_signal") {
            SignalKind::RwSignal
        } else if expr.starts_with("create_memo") {
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
            });
        }
    }

    signals
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_rw_signal() {
        let s = scan_signals("let count = create_rw_signal(0i32);");
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].name, "count");
        assert_eq!(s[0].kind, SignalKind::RwSignal);
    }

    #[test]
    fn detects_memo() {
        let s = scan_signals("let double = create_memo(move |_| count.get() * 2);");
        assert_eq!(s[0].name, "double");
        assert_eq!(s[0].kind, SignalKind::Memo);
    }

    #[test]
    fn ignores_plain_let() {
        let s = scan_signals("let x = 5;");
        assert!(s.is_empty());
    }
}
