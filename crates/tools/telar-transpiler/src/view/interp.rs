//! Text interpolation and color resolution for the view emitters.

use crate::style::hex_to_color_expr;
use telar_project::naming::is_ident;

use super::ViewGen;
use super::expr_marker;
use super::signals::{
    Segment, parse_interpolation, rust_str, substitute_reads, substitute_reads_spanned,
};

/// A quoted string read as text: `{…}` holes formatted into it, and `{{`/`}}` its braces.
pub(super) enum Interpolated {
    /// No hole: the string as it reads, braces unescaped.
    Literal(String),
    Formatted {
        format: String,
        /// One `let` per capture a closure around `format` must copy in rather than consume.
        clones: Vec<String>,
        /// Whether a hole reads state, so a value built from it has to be re-read to follow it.
        reactive: bool,
    },
}

impl Interpolated {
    pub(super) fn closure(&self, literal: impl FnOnce(&str) -> String) -> String {
        match self {
            Interpolated::Literal(text) => format!("|| {}", literal(text)),
            Interpolated::Formatted { format, clones, .. } if clones.is_empty() => {
                format!("move || {format}")
            }
            Interpolated::Formatted { format, clones, .. } => {
                format!("{{ {} move || {format} }}", clones.join(" "))
            }
        }
    }
}

impl ViewGen<'_> {
    /// `content` read as text, starting at source byte `content_start`. A `capturing` caller puts the `format!` inside a `move` closure, so each `[logic]` binding and `props` place a hole reads is copied in wherever consuming it would leave another reader without it; a caller formatting once, in place, borrows and copies nothing.
    pub(super) fn interpolation(
        &self,
        content: &str,
        content_start: usize,
        capturing: bool,
    ) -> Interpolated {
        let segments = parse_interpolation(content);
        let escaped_at = content
            .find(['"', '\\', '\n', '\t', '\r'])
            .unwrap_or(usize::MAX);
        let mut fmt = String::new();
        let mut literal = String::new();
        let mut args = Vec::new();
        let mut clones = Vec::new();
        let mut holes: Vec<&str> = Vec::new();
        for seg in &segments {
            match seg {
                Segment::Literal(text) => {
                    literal.push_str(text);
                    fmt.push_str(&text.replace('{', "{{").replace('}', "}}"));
                }
                Segment::Expr { text, byte_offset } => {
                    fmt.push_str("{}");
                    // The parser hands the string back unescaped, so past the first escape every offset is out by the bytes it dropped.
                    let start = (escaped_at >= byte_offset + text.len())
                        .then_some(content_start + byte_offset);
                    args.push(match capturing {
                        true => self.render_captured_expr(text, start, &mut clones),
                        false => self.render_interp_expr(text, start),
                    });
                    holes.push(text);
                }
            }
        }
        if holes.is_empty() {
            return Interpolated::Literal(literal);
        }
        let reactive = holes
            .iter()
            .any(|hole| hole.contains('$') || self.reads_reactive_context(hole));
        if capturing {
            let scoped = super::signals::unscoped(&holes);
            clones.extend(
                self.captured_names(&scoped)
                    .iter()
                    .map(|name| super::signals::clone_statement(name)),
            );
        }
        Interpolated::Formatted {
            format: format!("format!({}, {})", rust_str(&fmt), args.join(", ")),
            clones,
            reactive,
        }
    }

    /// A hole of a string a `move` closure formats: each `props` place it reads that another reader still needs is copied into a binding of its own ahead of the closure, so the closure takes that copy and leaves the rest of `props` — whatever `[logic]` moved out of it included — alone.
    fn render_captured_expr(
        &self,
        expr: &str,
        raw_start: Option<usize>,
        clones: &mut Vec<String>,
    ) -> String {
        let trimmed = expr.trim();
        let copied: Vec<(usize, usize)> = crate::rust::field_places(trimmed, "props")
            .into_iter()
            .filter(|&(start, end)| {
                let place = crate::rust::place_text(&trimmed[start..end]);
                self.must_copy_capture(self.reads.place(&place))
            })
            .collect();
        if copied.is_empty() {
            return self.render_interp_expr(expr, raw_start);
        }
        let lead = expr.len() - expr.trim_start().len();
        let marker = |start: usize, place: &str| {
            raw_start
                .map(|at| expr_marker(at + lead + start, place.len()))
                .unwrap_or_default()
        };
        let rewritten = super::signals::copy_places(trimmed, &copied, marker, clones);
        let body = match rewritten.contains('$') {
            true => substitute_reads(&rewritten, self.theme_access),
            false => rewritten,
        };
        format_arg(body)
    }

    /// Whether `expr` reads reactive state without a `$` saying so: a `use_…()` hook read through (`use_workbench_tokens().border`), or a `[logic]` signal read with `.get()` or `.with(…)`. Passing a hook's or a signal's handle on is not a read; the handle is what the callee follows.
    pub(super) fn reads_reactive_context(&self, expr: &str) -> bool {
        let bytes = expr.as_bytes();
        let follows = |at: usize, len: usize, rest: &[&str]| {
            let after = expr[at + len..].trim_start();
            rest.iter().any(|r| after.starts_with(r))
        };
        let hook_read = (0..bytes.len()).any(|at| {
            expr[at..].starts_with("use_")
                && (at == 0 || !telar_project::naming::is_ident_byte(bytes[at - 1]))
                && {
                    let name_len = expr[at..]
                        .bytes()
                        .take_while(|b| telar_project::naming::is_ident_byte(*b))
                        .count();
                    let after = expr[at + name_len..].trim_start();
                    let after = match after.strip_prefix("::<") {
                        Some(generic) => generic.find('>').map_or("", |end| &generic[end + 1..]),
                        None => after,
                    };
                    after.starts_with('(') && closes_into_read(after)
                }
        });
        hook_read
            || self.signals.iter().any(|signal| {
                crate::lexer::ident_positions(expr, signal)
                    .any(|at| follows(at, signal.len(), &[".get(", ".with("]))
            })
    }

    /// The runtime catalog-lookup expression for an i18n key written in markup (`t"key"`). Reads the active locale reactively, so wrapping it in a `move ||` content/label closure makes the widget live-switch on a language change. Markup keys take no arguments (parameterized strings use the Rust `t!` macro).
    pub(super) fn i18n_lookup(&self, key: &str) -> String {
        format!(
            "telar::i18n::translate(&{}, {}, &[])",
            telar_project::I18N_CATALOG_PATH,
            rust_str(key)
        )
    }

    /// Renders an interpolation expression: a `$ident` reactive read becomes `ident.get()`; a `$`-free expression is emitted verbatim (a plain value). `raw_start` is the source byte offset of the raw (untrimmed) expression text, or `None` where an escape before it makes that unknowable; an [`expr_marker`] is emitted right before a verbatim (`$`-free) expression so the analyzer can complete inside it.
    fn render_interp_expr(&self, expr: &str, raw_start: Option<usize>) -> String {
        let trimmed = expr.trim();
        if trimmed.is_empty() {
            return format!("{{ {expr} }}");
        }
        let lead = expr.len() - expr.trim_start().len();
        let Some(raw_start) = raw_start else {
            let body = match trimmed.contains('$') {
                true => substitute_reads(trimmed, self.theme_access),
                false => trimmed.to_string(),
            };
            return format_arg(body);
        };
        // The expression around a `$` is rewritten, so it carries no span of its own — but each identifier inside it is copied byte for byte and keeps one, which is what a cursor on `$name` resolves through.
        if trimmed.contains('$') {
            return format_arg(substitute_reads_spanned(
                trimmed,
                raw_start + lead,
                self.theme_access,
            ));
        }
        let marker = expr_marker(raw_start + lead, trimmed.len());
        format_arg(format!("{marker}{trimmed}"))
    }

    /// Resolves a color reference: an inline hex value, a `$signal` read, a computed reactive expression, a CSS keyword, a `Color::*` literal, or any other expression, emitted as written.
    ///
    /// Lookup order:
    /// 1. Inline hex → static expression.
    /// 2. A bare `$ident` → `ident.get()`, a reactive read of a `RwSignal<Color>` (or compatible) in scope.
    /// 3. A computed expression — a call, method chain, or arithmetic that yields a `Color`, recognized by a `(` or an embedded `$` beyond a bare handle (e.g. `chip_fill($snapshot, id)` for state-driven paint). `$signal` reads are made reactive via `substitute_reads`; the rest is emitted verbatim. For (2) and (3) the enclosing `move` paint closure takes its captures by [`ViewGen::owning`], so the color re-reads and the outer handle stays usable.
    /// 4. `Color::*` literal / CSS keyword → static expression.
    /// 5. Anything else, a `[logic]` binding included → verbatim: a `[style]` constant, or rustc catches the missing symbol.
    ///
    /// A `$theme.x` is one of (3), so it reads the theme wherever it is written: `theme.get().x`, or a library's `::telar::use_theme_tokens().x()`.
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
        // Before the `Color::`/keyword arms, so a state-driven paint like `chip_fill($snapshot, id)` is emitted whole rather than treated as a token.
        if v.contains('(') || v.contains('$') {
            return substitute_reads(v, self.theme_access);
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
}

fn closes_into_read(call: &str) -> bool {
    let mut depth = 0i32;
    for (i, c) in call.char_indices() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                depth -= 1;
                if depth == 0 {
                    let rest = call[i + 1..].trim_start();
                    return rest.starts_with('.') && !rest.starts_with("..");
                }
            }
            _ => {}
        }
    }
    false
}

impl ViewGen<'_> {
    pub(super) fn text_closure(&self, text: &str, start: usize) -> String {
        self.interpolation(text, start, true).closure(rust_str)
    }
}

/// A hole as a `format!` argument: as written, since `format!` borrows what it is given, where a block around it would move a place like `props.detail` out of the closure that formats it. A hole reading `name = …` is parenthesised, so it stays a value rather than naming an argument.
fn format_arg(hole: String) -> String {
    let unmarked = hole.rsplit(super::SRC_EXPR_CLOSE).next().unwrap_or(&hole);
    let named = unmarked
        .split_once('=')
        .is_some_and(|(name, rest)| is_ident(name.trim()) && !rest.starts_with('='));
    match named {
        true => format!("({hole})"),
        false => hole,
    }
}
