//! What a Rust expression names, read by parsing it.
//!
//! Every value in `[view]` is Rust now, and two questions about one keep coming up: which names in it are *free* — bound by the surrounding scope rather than by the expression itself — and, given that, which of those a `move` closure has to clone in.
//!
//! Both used to be answered by scanning the text for a word. That cannot tell a binding from a field, a method, or a path segment: with a local named `x`, `seat(&desk, id).x` looked like a use of it, and the emitted clone was a compile error against generated code. `syn` answers honestly, and it is the same parser rustc's front end uses, so the two agree by construction.

use std::collections::HashSet;

use syn::visit::Visit;

/// The free identifiers of `expr` — every single-segment path it names that it does not itself bind — or `None` when the text is not a Rust expression.
///
/// `None` is not a failure to report: an attribute value can be half-written mid-keystroke, and a caller that cannot parse falls back to its own scan rather than dropping a capture the closure needs.
pub(crate) fn free_idents(expr: &str) -> Option<Vec<String>> {
    let parsed: syn::Expr = Fallback::forced(|| syn::parse_str(expr)).ok()?;
    let mut visitor = FreeIdents {
        bound: vec![HashSet::new()],
        found: Vec::new(),
    };
    visitor.visit_expr(&parsed);
    Some(visitor.found)
}

/// Parses with proc-macro2's own lexer for the length of one call.
///
/// The transpiler runs inside the `app!` proc macro, where proc-macro2 hands `parse_str` to *rustc's* lexer — and rustc reports a lexer error by emitting it, not by returning it. So a `.rsx` holding `stroke_width:'0 0 2 0'` or a sentence with an `I"` in it aborted the build with a diagnostic about the macro call, from a parse whose only purpose was to ask a question and accept "I don't know" for an answer. Forced to the fallback, the same parse returns `Err` and the caller takes its other branch.
///
/// Restored on drop, so the generated code `app!` parses back at the end keeps the compiler's real spans.
struct Fallback;

impl Fallback {
    fn forced<T>(parse: impl FnOnce() -> T) -> T {
        let _guard = Fallback;
        proc_macro2::fallback::force();
        parse()
    }
}

impl Drop for Fallback {
    fn drop(&mut self) {
        proc_macro2::fallback::unforce();
    }
}

struct FreeIdents {
    /// One frame per binding scope, innermost last: a closure's parameters, a block's `let`s.
    bound: Vec<HashSet<String>>,
    found: Vec<String>,
}

impl FreeIdents {
    fn is_bound(&self, name: &str) -> bool {
        self.bound.iter().any(|frame| frame.contains(name))
    }

    fn bind(&mut self, name: String) {
        if let Some(frame) = self.bound.last_mut() {
            frame.insert(name);
        }
    }

    fn scoped(&mut self, f: impl FnOnce(&mut Self)) {
        self.bound.push(HashSet::new());
        f(self);
        self.bound.pop();
    }

    /// Every name a pattern introduces. `syn` gives `PatIdent` for each one, including inside tuples, slices and struct patterns, so the walk is the whole of it.
    fn bind_pattern(&mut self, pat: &syn::Pat) {
        struct Binder<'a>(&'a mut FreeIdents);
        impl<'ast> Visit<'ast> for Binder<'_> {
            fn visit_pat_ident(&mut self, pat: &'ast syn::PatIdent) {
                self.0.bind(pat.ident.to_string());
                syn::visit::visit_pat_ident(self, pat);
            }
        }
        Binder(self).visit_pat(pat);
    }
}

impl<'ast> Visit<'ast> for FreeIdents {
    fn visit_expr_path(&mut self, node: &'ast syn::ExprPath) {
        // A multi-segment path names an item, never a local, and `self` is the receiver rather than a capture.
        if node.qself.is_none()
            && let Some(ident) = node.path.get_ident()
        {
            let name = ident.to_string();
            if !self.is_bound(&name) && !self.found.contains(&name) {
                self.found.push(name);
            }
        }
        syn::visit::visit_expr_path(self, node);
    }

    /// `a.x` names `a`; `x` is a field of whatever `a` is.
    fn visit_expr_field(&mut self, node: &'ast syn::ExprField) {
        self.visit_expr(&node.base);
    }

    /// `a.f(b)` names `a` and `b`; `f` is a method resolved against `a`'s type.
    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        self.visit_expr(&node.receiver);
        for arg in &node.args {
            self.visit_expr(arg);
        }
    }

    /// `S { x: v }` names `v`; `x` is a field of `S`.
    fn visit_expr_struct(&mut self, node: &'ast syn::ExprStruct) {
        for field in &node.fields {
            self.visit_expr(&field.expr);
        }
        if let Some(rest) = &node.rest {
            self.visit_expr(rest);
        }
    }

    fn visit_expr_closure(&mut self, node: &'ast syn::ExprClosure) {
        self.scoped(|this| {
            for input in &node.inputs {
                this.bind_pattern(input);
            }
            this.visit_expr(&node.body);
        });
    }

    /// A block binds as it goes: a `let` is in scope for the statements after it, not before.
    fn visit_block(&mut self, node: &'ast syn::Block) {
        self.scoped(|this| {
            for stmt in &node.stmts {
                this.visit_stmt(stmt);
            }
        });
    }

    fn visit_local(&mut self, node: &'ast syn::Local) {
        if let Some(init) = &node.init {
            self.visit_expr(&init.expr);
            if let Some((_, diverge)) = &init.diverge {
                self.visit_expr(diverge);
            }
        }
        self.bind_pattern(&node.pat);
    }

    fn visit_arm(&mut self, node: &'ast syn::Arm) {
        self.scoped(|this| {
            this.bind_pattern(&node.pat);
            this.visit_expr(&node.body);
        });
    }

    fn visit_expr_for_loop(&mut self, node: &'ast syn::ExprForLoop) {
        self.visit_expr(&node.expr);
        self.scoped(|this| {
            this.bind_pattern(&node.pat);
            this.visit_block(&node.body);
        });
    }

    /// A macro's tokens are not parsed as an expression, so nothing is claimed about what they name — the caller's scan covers `format!("{x}")` and its kin.
    fn visit_macro(&mut self, _: &'ast syn::Macro) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn idents(expr: &str) -> Vec<String> {
        free_idents(expr).unwrap_or_else(|| panic!("`{expr}` did not parse"))
    }

    #[test]
    fn a_field_a_method_and_a_path_segment_are_not_bindings() {
        assert_eq!(idents("seat(&desk, id).x"), ["seat", "desk", "id"]);
        assert_eq!(idents("crate::scale::md()"), Vec::<String>::new());
        assert_eq!(idents("value.clamp(lo, hi)"), ["value", "lo", "hi"]);
        assert_eq!(idents("Style { pad, ..base }"), ["pad", "base"]);
    }

    #[test]
    fn a_closure_parameter_shadows_the_scope_it_sits_in() {
        assert_eq!(
            idents("items.iter().map(|x| x + offset)"),
            ["items", "offset"]
        );
        assert_eq!(idents("|n| { let m = n * 2; m + k }"), ["k"]);
    }

    #[test]
    fn a_pattern_binds_every_name_it_introduces() {
        assert_eq!(
            idents("match slot { Some((a, b)) => a + b, None => fallback }"),
            ["slot", "fallback"]
        );
    }

    /// `$` is the markup's sugar and not Rust, so a value still carrying one has to be substituted before it can be read — saying so is what keeps the caller's fallback scan reachable.
    #[test]
    fn text_that_is_not_an_expression_says_so_instead_of_guessing() {
        assert_eq!(free_idents("$count.get()"), None);
        assert_eq!(free_idents("col gap:8"), None);
        assert_eq!(idents("12px"), Vec::<String>::new());
    }
}
