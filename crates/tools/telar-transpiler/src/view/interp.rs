//! Text interpolation and color resolution for the view emitters.

use crate::style::hex_to_color_expr;
use telar_project::naming::is_ident;

use super::ViewGen;
use super::expr_marker;
use super::signals::{
    Segment, parse_interpolation, rust_str, substitute_reads, substitute_reads_spanned,
};

impl ViewGen<'_> {
    /// Builds the `content_fn` closure for a text node, handling `{...}` interpolation. `content_start` is the source byte offset of `content`, used to tag each interpolated expression with its source span.
    pub fn interpolate_content(&self, content: &str, content_start: usize) -> String {
        let segments = parse_interpolation(content);
        if segments.iter().all(|s| matches!(s, Segment::Literal(_))) {
            return format!("|| {}.to_string()", rust_str(content));
        }

        let mut fmt = String::new();
        let mut args = Vec::new();
        for seg in &segments {
            match seg {
                Segment::Literal(text) => {
                    fmt.push_str(&text.replace('{', "{{").replace('}', "}}"));
                }
                Segment::Expr { text, byte_offset } => {
                    fmt.push_str("{}");
                    args.push(self.render_interp_expr(text, content_start + byte_offset));
                }
            }
        }

        let args_joined = args.join(", ");
        format!("move || format!({}, {args_joined})", rust_str(&fmt))
    }

    /// The runtime catalog-lookup expression for an i18n key written in markup (`t"key"`). Reads the active locale reactively, so wrapping it in a `move ||` content/label closure makes the widget live-switch on a language change. Markup keys take no arguments (parameterized strings use the Rust `t!` macro).
    pub(super) fn i18n_lookup(&self, key: &str) -> String {
        format!(
            "telar::i18n::translate(&{}, {}, &[])",
            telar_project::I18N_CATALOG_PATH,
            rust_str(key)
        )
    }

    /// Renders an interpolation expression: a `$ident` reactive read becomes `ident.get()`; a `$`-free expression is emitted verbatim (a plain value). `raw_start` is the source byte offset of the raw (untrimmed) expression text; an [`expr_marker`] is emitted right before a verbatim (`$`-free) expression so the analyzer can complete inside it.
    fn render_interp_expr(&self, expr: &str, raw_start: usize) -> String {
        let trimmed = expr.trim();
        if trimmed.is_empty() {
            return format!("{{ {expr} }}");
        }
        // The expression around a `$` is rewritten, so it carries no span of its own — but each identifier inside it is copied byte for byte and keeps one, which is what a cursor on `$name` resolves through.
        if trimmed.contains('$') {
            let lead = expr.len() - expr.trim_start().len();
            return format!(
                "{{ {} }}",
                substitute_reads_spanned(trimmed, raw_start + lead)
            );
        }
        // Nor an expression carrying a string literal: the parser hands content back unescaped, so a `"` here was `\"` in the `.rsx` and every offset after it is off by one. The span is dropped rather than made to lie.
        if trimmed.contains('"') {
            return format!("{{ {trimmed} }}");
        }
        let lead = expr.len() - expr.trim_start().len();
        let marker = expr_marker(raw_start + lead, trimmed.len());
        if is_ident(trimmed) {
            format!("{marker}{trimmed}")
        } else {
            format!("{{ {marker}{trimmed} }}")
        }
    }

    /// Resolves a color reference: an inline hex value, a `$signal` read, a computed reactive expression, a CSS keyword, a `Color::*` literal, a `[style]`-declared local constant, or a theme field.
    ///
    /// Lookup order:
    /// 1. Inline hex → static expression.
    /// 2. A bare `$ident` → `ident.get()`, a reactive read of a `RwSignal<Color>` (or compatible) in scope.
    /// 3. A computed expression — a call, method chain, or arithmetic that yields a `Color`, recognized by a `(` or an embedded `$` beyond a bare handle (e.g. `chip_fill($snapshot, id)` for state-driven paint). `$signal` reads are made reactive via `substitute_reads`; the rest is emitted verbatim. For (2) and (3) the caller clones any captured signal into the enclosing `move` paint closure (see `wrap_signal_clones`), so the color re-reads and the outer handle stays usable.
    /// 4. `Color::*` literal / CSS keyword → static expression.
    /// 5. A `[logic]` binding of that name → the binding itself, so a local shadows a same-named token the way it would in Rust.
    /// 6. `theme_type` set → `use_theme::<T>().field` (reactive) for every named color, including `[style]`-declared ones, so runtime theme switching takes effect; use inline hex for a true non-theme one-off.
    /// 7. No `theme_type` → file-local `COLOR_*` constant (declared in `[style]`, or rustc catches the missing symbol if undeclared).
    ///
    /// `at` is where `value` begins in the `.rsx`, or `None` for a value that is not an attribute of its own — a gradient stop, or a prop a component already parsed — and so has no position to record. Given one, a value that reaches the output byte for byte carries a source marker, which is what puts `color:(row_ink(state, index))` within reach of a rename. Cases 1, 2, 4 and the keywords are rewritten on the way, so there is no span to record and the cursor falls back to its line. The position is asked for rather than inferred because a caller holding only the text cannot be told apart from one that forgot to pass it, and forgetting costs a rename that refuses with no way to see why.
    pub(super) fn color_expr(&self, value: &str, at: Option<usize>) -> String {
        let trimmed = value.trim();
        let (inner, delimiters) = match super::redundant_parens(trimmed) {
            Some(inner) => (inner, 1),
            None => (trimmed, 0),
        };
        let emitted = self.color_value(inner);
        let Some(at) = at.filter(|_| emitted == inner) else {
            return emitted;
        };
        let lead = value.len() - value.trim_start().len() + delimiters;
        format!("{}{emitted}", expr_marker(at + lead, inner.len()))
    }

    fn color_value(&self, v: &str) -> String {
        if v.starts_with('#') {
            return hex_to_color_expr(v);
        }
        if let Some(ident) = v.strip_prefix('$')
            && is_ident(ident)
        {
            return format!("{ident}.get()");
        }
        // Before the `Color::`/keyword arms, so a state-driven paint like `chip_fill($snapshot, id)` is emitted whole rather than treated as a token.
        if v.contains('(') || v.contains('$') {
            return substitute_reads(v);
        }
        if v.starts_with("Color::") {
            return v.to_string();
        }
        if v == "transparent" {
            return "Color::TRANSPARENT".to_string();
        }
        if self.is_local(v) {
            return v.to_string();
        }
        v.to_string()
    }

    /// Whether codegen resolves any color through `use_theme`, requiring the import.
    pub fn uses_theme(&self) -> bool {
        self.theme_type.is_some()
    }
}
