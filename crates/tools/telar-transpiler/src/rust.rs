//! What Rust code names, read by parsing it.
//!
//! Every value in `[view]` is Rust now, and the same questions keep coming up about it and about `[logic]`: which names in it are *free* — bound by the surrounding scope rather than by the code itself — and, given that, which of those a `move` closure has to clone in.
//!
//! Both used to be answered by scanning the text for a word. That cannot tell a binding from a field, a method, or a path segment: with a local named `x`, `seat(&desk, id).x` looked like a use of it, and the emitted clone was a compile error against generated code. `syn` answers honestly, and it is the same parser rustc's front end uses, so the two agree by construction.

use std::collections::HashSet;

use proc_macro2::{Ident, Spacing, TokenStream, TokenTree};
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::visit::Visit;

use crate::lexer::{format_arg_positions, ident_positions};

/// The free identifiers of `expr` — every single-segment path it names that it does not itself bind — or `None` when the text is not a Rust expression.
///
/// `None` is not a failure to report: an attribute value can be half-written mid-keystroke, and a caller that cannot parse falls back to its own scan rather than dropping a capture the closure needs.
pub(crate) fn free_idents(expr: &str) -> Option<Vec<String>> {
    Fallback::forced(|| {
        let parsed: syn::Expr = syn::parse_str(expr).ok()?;
        let mut found: Vec<String> = Vec::new();
        ScopeWalk::new(unraw, |ident: &Ident, _| {
            let name = ident.to_string();
            if !found.contains(&name) {
                found.push(name);
            }
        })
        .visit_expr(&parsed);
        Some(found)
    })
}

/// A read of one of the names asked about, made from inside a `move` closure or `async move` block and naming the binding from outside the code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MovedRead {
    /// Byte offset of the name in the code.
    pub(crate) offset: usize,
    pub(crate) name: String,
    /// Written as field-init shorthand (`Config { name }`), so the field has to be spelled out before the value can be renamed.
    pub(crate) shorthand: bool,
}

/// Every read of one of `names` that a `move` closure or `async move` block in `code` — one or more `[logic]` statements — captures from the scope around it, in source order; `None` when `code` does not parse.
///
/// A field, a method, a path segment, a macro name, a label, an explicit format argument's name (`x = …`) and a binding the code introduces itself (a closure parameter, a `let`, a pattern) are not reads, so none of them come back. A name a format string captures (`format!("{x}")`) is one, and its offset points inside the string, where renaming it keeps the capture and the output.
///
/// `syn` gives no source positions without proc-macro2's `span-locations`, which makes every fallback parse in the process keep its text for good. Instead each candidate occurrence is renamed to a numbered marker before parsing, so an identifier the walk meets says which occurrence it is.
pub(crate) fn moved_reads(code: &str, names: &[&str]) -> Option<Vec<MovedRead>> {
    let mut occurrences: Vec<(usize, &str)> = names
        .iter()
        .flat_map(|name| {
            ident_positions(code, name)
                .chain(format_arg_positions(code, name))
                .map(move |at| (at, *name))
        })
        .collect();
    if occurrences.is_empty() {
        return Some(Vec::new());
    }
    occurrences.sort_unstable();
    let marker = (0..)
        .map(|n| format!("__rsx_occurrence{n}_"))
        .find(|marker| !code.contains(marker.as_str()))?;
    let mut marked = String::with_capacity(code.len() + occurrences.len() * (marker.len() + 4));
    let mut copied = 0;
    for (index, &(at, name)) in occurrences.iter().enumerate() {
        marked.push_str(&code[copied..at]);
        marked.push_str(&format!("{name}{marker}{index}"));
        copied = at + name.len();
    }
    marked.push_str(&code[copied..]);

    let occurrence = |ident: &Ident| -> Option<usize> {
        let text = ident.to_string();
        let (_, index) = unraw_str(&text).rsplit_once(marker.as_str())?;
        index.parse().ok()
    };
    let unmarked = |ident: &Ident| -> String {
        let text = ident.to_string();
        let name = unraw_str(&text);
        name.rsplit_once(marker.as_str())
            .filter(|(_, index)| index.parse::<usize>().is_ok())
            .map_or(name, |(base, _)| base)
            .to_string()
    };

    Fallback::forced(|| {
        let stmts = syn::Block::parse_within.parse_str(&marked).ok()?;
        let mut reads = Vec::new();
        let mut walk = ScopeWalk::new(unmarked, |ident: &Ident, read: Read| {
            if !read.moved {
                return;
            }
            if let Some(&(offset, name)) = occurrence(ident).and_then(|i| occurrences.get(i)) {
                reads.push(MovedRead {
                    offset,
                    name: name.to_string(),
                    shorthand: read.shorthand,
                });
            }
        });
        for stmt in &stmts {
            walk.visit_stmt(stmt);
        }
        reads.sort_by_key(|read| read.offset);
        reads.dedup_by_key(|read| read.offset);
        Some(reads)
    })
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

fn unraw_str(name: &str) -> &str {
    name.strip_prefix("r#").unwrap_or(name)
}

fn unraw(ident: &Ident) -> String {
    unraw_str(&ident.to_string()).to_string()
}

/// Every name a pattern introduces. `syn` gives a `PatIdent` for each one, including inside tuples, slices and struct patterns, so the walk is the whole of it — short of a match guard, whose expression binds nothing.
fn pattern_bindings(pat: &syn::Pat) -> Vec<&Ident> {
    struct Binder<'ast>(Vec<&'ast Ident>);
    impl<'ast> Visit<'ast> for Binder<'ast> {
        fn visit_pat_ident(&mut self, pat: &'ast syn::PatIdent) {
            self.0.push(&pat.ident);
            syn::visit::visit_pat_ident(self, pat);
        }

        fn visit_pat_guard(&mut self, pat: &'ast syn::PatGuard) {
            self.visit_pat(&pat.pat);
        }
    }
    let mut binder = Binder(Vec::new());
    binder.visit_pat(pat);
    binder.0
}

/// The names a `[view]` `let` binds, or `None` when the text is not a `let` statement.
///
/// A view `let` declares *inside* the closure the clone pass wraps around it, so a `$rect` written under `let rect = …` names that binding — and a prelude that clones `rect` above the closure names something which is not there yet.
pub(crate) fn let_bindings(source: &str) -> Option<Vec<String>> {
    let source = source.trim().trim_end_matches(';');
    let stmt: syn::Stmt = Fallback::forced(|| syn::parse_str(&format!("{source};"))).ok()?;
    match stmt {
        syn::Stmt::Local(local) => Some(
            pattern_bindings(&local.pat)
                .into_iter()
                .map(Ident::to_string)
                .collect(),
        ),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy)]
struct Read {
    moved: bool,
    shorthand: bool,
}

/// Walks Rust code reporting every identifier it reads from the scope around it, and whether a `move` captures that read.
struct ScopeWalk<N, F> {
    /// One frame per binding scope, innermost last: a closure's parameters, a block's `let`s.
    bound: Vec<HashSet<String>>,
    moving: usize,
    name: N,
    read: F,
}

impl<N: Fn(&Ident) -> String, F: FnMut(&Ident, Read)> ScopeWalk<N, F> {
    fn new(name: N, read: F) -> Self {
        Self {
            bound: vec![HashSet::new()],
            moving: 0,
            name,
            read,
        }
    }

    fn report(&mut self, ident: &Ident, shorthand: bool) {
        let name = (self.name)(ident);
        if !self.bound.iter().any(|frame| frame.contains(&name)) {
            let moved = self.moving > 0;
            (self.read)(ident, Read { moved, shorthand });
        }
    }

    fn scoped(&mut self, f: impl FnOnce(&mut Self)) {
        self.bound.push(HashSet::new());
        f(self);
        self.bound.pop();
    }

    fn moving(&mut self, capture: bool, f: impl FnOnce(&mut Self)) {
        self.moving += usize::from(capture);
        f(self);
        self.moving -= usize::from(capture);
    }

    fn bind_pattern(&mut self, pat: &syn::Pat) {
        for ident in pattern_bindings(pat) {
            let name = (self.name)(ident);
            if let Some(frame) = self.bound.last_mut() {
                frame.insert(name);
            }
        }
    }

    /// A macro's tokens are not Rust until the macro says what they are, so they are read the ways macros most often use them: arguments like `format!`'s, where `name = value` names a parameter rather than a binding; statements; and, failing both, any identifier that is not a field, a path segment or another macro.
    fn visit_macro_tokens(&mut self, mac: &syn::Macro) {
        if let Ok(args) =
            mac.parse_body_with(Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated)
        {
            for arg in &args {
                match arg {
                    syn::Expr::Assign(named) if single_ident(&named.left).is_some() => {
                        self.visit_expr(&named.right);
                    }
                    other => self.visit_expr(other),
                }
            }
        } else if let Ok(stmts) = mac.parse_body_with(syn::Block::parse_within) {
            self.scoped(|this| {
                for stmt in &stmts {
                    this.visit_stmt(stmt);
                }
            });
        } else {
            self.visit_token_stream(mac.tokens.clone());
        }
        self.visit_format_captures(mac);
    }

    /// `format!("{x}")` reads `x` without naming it anywhere a parser looks — but only in a macro whose string really is a format string, and only where the call does not pass an `x = …` of its own.
    fn visit_format_captures(&mut self, mac: &syn::Macro) {
        let Some(at) = mac
            .path
            .segments
            .last()
            .and_then(|segment| format_string_at(&segment.ident.to_string()))
        else {
            return;
        };
        let args = top_level_args(mac.tokens.clone());
        let lone_string = |arg: &Vec<TokenTree>| match arg.as_slice() {
            [TokenTree::Literal(literal)] => {
                syn::parse2::<syn::LitStr>(TokenTree::Literal(literal.clone()).into()).ok()
            }
            _ => None,
        };
        let format = match at {
            FormatAt::Arg(index) => args.get(index).and_then(lone_string),
            FormatAt::FirstString => args.iter().find_map(lone_string),
        };
        let Some(format) = format else {
            return;
        };
        let explicit: Vec<String> = args
            .iter()
            .filter_map(|arg| match arg.as_slice() {
                [TokenTree::Ident(name), TokenTree::Punct(eq), ..]
                    if eq.as_char() == '=' && eq.spacing() == Spacing::Alone =>
                {
                    Some((self.name)(name))
                }
                _ => None,
            })
            .collect();
        for name in implicit_format_args(&format.value()) {
            let Ok(ident) = syn::parse_str::<Ident>(name) else {
                continue;
            };
            if !explicit.contains(&(self.name)(&ident)) {
                self.report(&ident, false);
            }
        }
    }

    fn visit_token_stream(&mut self, tokens: TokenStream) {
        let tokens: Vec<TokenTree> = tokens.into_iter().collect();
        for (i, token) in tokens.iter().enumerate() {
            match token {
                TokenTree::Group(group) => self.visit_token_stream(group.stream()),
                TokenTree::Ident(ident) if names_a_value(&tokens, i) => self.report(ident, false),
                _ => {}
            }
        }
    }
}

fn single_ident(expr: &syn::Expr) -> Option<&Ident> {
    match expr {
        syn::Expr::Path(path) if path.qself.is_none() => path.path.get_ident(),
        _ => None,
    }
}

enum FormatAt {
    Arg(usize),
    /// `tracing` and `log` put fields and `target:` ahead of the message, so it is the first argument that is a string and nothing else.
    FirstString,
}

fn format_string_at(macro_name: &str) -> Option<FormatAt> {
    Some(match macro_name {
        "format" | "format_args" | "print" | "println" | "eprint" | "eprintln" | "panic"
        | "unreachable" | "todo" | "unimplemented" | "anyhow" | "bail" | "format_err" => {
            FormatAt::Arg(0)
        }
        "write" | "writeln" | "assert" | "debug_assert" | "ensure" => FormatAt::Arg(1),
        "assert_eq" | "assert_ne" | "debug_assert_eq" | "debug_assert_ne" => FormatAt::Arg(2),
        "trace" | "debug" | "info" | "warn" | "error" | "event" => FormatAt::FirstString,
        _ => return None,
    })
}

fn top_level_args(tokens: TokenStream) -> Vec<Vec<TokenTree>> {
    let mut args = vec![Vec::new()];
    for token in tokens {
        match &token {
            TokenTree::Punct(p) if p.as_char() == ',' => args.push(Vec::new()),
            _ => args.last_mut().expect("starts with one").push(token),
        }
    }
    args.retain(|arg| !arg.is_empty());
    args
}

/// The names a format string captures from scope: `{x}`, `{x:?}`, and a `x$` width or precision. `{{` is a brace, and `{0}`/`{}` are positional.
fn implicit_format_args(format: &str) -> Vec<&str> {
    let mut names = Vec::new();
    let mut rest = format;
    while let Some(open) = rest.find('{') {
        let inside = &rest[open + 1..];
        if let Some(after_escape) = inside.strip_prefix('{') {
            rest = after_escape;
            continue;
        }
        let Some(close) = inside.find('}') else {
            break;
        };
        let (arg, spec) = inside[..close]
            .split_once(':')
            .unwrap_or((&inside[..close], ""));
        if !arg.trim().is_empty() {
            names.push(arg.trim());
        }
        let mut start = None;
        for (i, ch) in spec.char_indices() {
            match start {
                None if ch == '_' || ch.is_alphabetic() => start = Some(i),
                Some(from) if !(ch == '_' || ch.is_alphanumeric()) => {
                    if ch == '$' {
                        names.push(&spec[from..i]);
                    }
                    start = None;
                }
                _ => {}
            }
        }
        rest = &inside[close + 1..];
    }
    names
}

fn names_a_value(tokens: &[TokenTree], i: usize) -> bool {
    let punct = |j: Option<usize>| match j.and_then(|j| tokens.get(j)) {
        Some(TokenTree::Punct(p)) => Some(p.as_char()),
        _ => None,
    };
    let before = |n: usize| punct(i.checked_sub(n));
    let after = |n: usize| punct(Some(i + n));
    let field = before(1) == Some('.') && before(2) != Some('.');
    let path_segment = (before(1) == Some(':') && before(2) == Some(':'))
        || (after(1) == Some(':') && after(2) == Some(':'));
    let macro_name =
        after(1) == Some('!') && matches!(tokens.get(i + 2), Some(TokenTree::Group(_)));
    !(field || path_segment || macro_name)
}

impl<'ast, N: Fn(&Ident) -> String, F: FnMut(&Ident, Read)> Visit<'ast> for ScopeWalk<N, F> {
    fn visit_expr_path(&mut self, node: &'ast syn::ExprPath) {
        // A multi-segment path names an item, never a local.
        if node.qself.is_none()
            && let Some(ident) = node.path.get_ident()
        {
            self.report(ident, false);
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

    /// `S { x: v }` names `v`; `x` is a field of `S`. `S { x }` names `x` in both roles at once.
    fn visit_expr_struct(&mut self, node: &'ast syn::ExprStruct) {
        for field in &node.fields {
            match (&field.member, field.colon_token, single_ident(&field.expr)) {
                (syn::Member::Named(_), None, Some(ident)) => self.report(ident, true),
                _ => self.visit_expr(&field.expr),
            }
        }
        if let Some(rest) = &node.rest {
            self.visit_expr(rest);
        }
    }

    fn visit_expr_closure(&mut self, node: &'ast syn::ExprClosure) {
        self.moving(node.capture.is_some(), |this| {
            this.scoped(|this| {
                for input in &node.inputs {
                    this.bind_pattern(input);
                }
                this.visit_expr(&node.body);
            });
        });
    }

    fn visit_expr_async(&mut self, node: &'ast syn::ExprAsync) {
        self.moving(node.capture.is_some(), |this| this.visit_block(&node.block));
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

    /// A `let` in a condition binds into the scope the condition opened — the `if` body, the `while` body, the rest of a let chain.
    fn visit_expr_let(&mut self, node: &'ast syn::ExprLet) {
        self.visit_expr(&node.expr);
        self.bind_pattern(&node.pat);
    }

    fn visit_expr_if(&mut self, node: &'ast syn::ExprIf) {
        self.scoped(|this| {
            this.visit_expr(&node.cond);
            this.visit_block(&node.then_branch);
        });
        if let Some((_, otherwise)) = &node.else_branch {
            self.visit_expr(otherwise);
        }
    }

    fn visit_expr_while(&mut self, node: &'ast syn::ExprWhile) {
        self.scoped(|this| {
            this.visit_expr(&node.cond);
            this.visit_block(&node.body);
        });
    }

    fn visit_arm(&mut self, node: &'ast syn::Arm) {
        self.scoped(|this| {
            this.bind_pattern(&node.pat);
            if let syn::Pat::Guard(guarded) = &node.pat {
                this.visit_expr(&guarded.guard);
            }
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

    /// A nested `fn`, `impl` or `const` cannot see the locals around it, so nothing in it reads them.
    fn visit_item(&mut self, _: &'ast syn::Item) {}

    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        self.visit_macro_tokens(node);
    }
}

#[cfg(test)]
#[path = "rust_test.rs"]
mod tests;
