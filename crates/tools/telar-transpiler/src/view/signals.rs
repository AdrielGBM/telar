//! Free helper functions shared across the view emitters: `$signal` substitution, interpolation parsing, and the paint/rect-style/gradient builders.

use std::fmt::Write;

use telar_parser::{Attr, ViewNode};

use crate::naming::contains_ident;
use crate::style::format_f32;

use super::expr_marker;

pub(super) enum Segment {
    Literal(String),
    /// An interpolated `{expr}`: the raw inner text plus the byte offset (within `content`) where it begins, used to map the verbatim expression back to the `.rsx` source.
    Expr {
        text: String,
        byte_offset: usize,
    },
}

/// Splits a string into literal and `{expr}` segments. Escaped braces `{{`/`}}` are treated as literal single braces.
pub(super) fn parse_interpolation(content: &str) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut literal = String::new();
    let mut chars = content.char_indices().peekable();

    while let Some((idx, c)) = chars.next() {
        match c {
            '{' if chars.peek().map(|&(_, c)| c) == Some('{') => {
                chars.next();
                literal.push('{');
            }
            '}' if chars.peek().map(|&(_, c)| c) == Some('}') => {
                chars.next();
                literal.push('}');
            }
            '{' => {
                if !literal.is_empty() {
                    segments.push(Segment::Literal(std::mem::take(&mut literal)));
                }
                let mut expr = String::new();
                // The expression text begins one byte past this `{`.
                let byte_offset = idx + c.len_utf8();
                for (_, ec) in chars.by_ref() {
                    if ec == '}' {
                        break;
                    }
                    expr.push(ec);
                }
                segments.push(Segment::Expr {
                    text: expr,
                    byte_offset,
                });
            }
            _ => literal.push(c),
        }
    }
    if !literal.is_empty() {
        segments.push(Segment::Literal(literal));
    }
    segments
}

/// Extracts the binding identifiers from a `for` pattern, ignoring tuple punctuation and the `_` wildcard. `(i, item)` -> `["i", "item"]`.
pub(super) fn pattern_idents(pattern: &str) -> Vec<String> {
    let mut idents = Vec::new();
    let mut current = String::new();
    for c in pattern.chars() {
        if c == '_' || c.is_ascii_alphanumeric() {
            current.push(c);
        } else if !current.is_empty() {
            idents.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        idents.push(current);
    }
    idents
        .into_iter()
        .filter(|i| {
            i != "_"
                && i.chars()
                    .next()
                    .is_some_and(|c| c == '_' || c.is_ascii_alphabetic())
        })
        .collect()
}

/// Renders a Rust string literal, escaping quotes, backslashes, and control chars. Newlines/tabs are
/// escaped (not emitted raw) so a decoded newline in `.rsx` content stays a single line in the generated
/// source — [`crate::view::resolve_source_map`] splits the body on `\n`, so a raw newline would desync it.
pub(super) fn rust_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// An [`expr_marker`] for a verbatim closure attribute value (one beginning with `|`), or an empty string otherwise. The value is emitted byte-for-byte after `move `, so the span maps directly.
pub(super) fn closure_marker(attr: Option<&Attr>) -> String {
    let Some(attr) = attr else {
        return String::new();
    };
    let trimmed = attr.value.trim_start();
    if !trimmed.starts_with('|') {
        return String::new();
    }
    let lead = attr.value.len() - trimmed.len();
    expr_marker(attr.value_start + lead, attr.value.trim().len())
}

/// The parser strips `on_press:` leaving `|| expr` or `|ev| expr`. Ensure the value is a closure; wrap bare expressions in a zero-arg closure. Then desugar a lone compound-assignment on a signal.
pub(super) fn normalize_closure(value: &str) -> String {
    let v = value.trim();
    let closure = if v.starts_with('|') {
        v.to_string()
    } else {
        format!("|| {{ {v} }}")
    };
    rewrite_compound_assign(&closure)
}

/// Sugar: a closure whose body is a single compound assignment on a signal (`|| $count += 1`, and the
/// single-statement block form `|| { $count += 1 }`) is rewritten to `|| $count.update(|__v| *__v += (1))`.
/// The `$` is kept so signal-cloning and [`substitute_handles`] still see the handle. Anything more
/// complex (multiple statements, no compound operator, not a signal target) is returned unchanged.
fn rewrite_compound_assign(closure: &str) -> String {
    // `closure` starts with `|` (guaranteed by `normalize_closure`); split off its `||` / `|args|` head.
    let Some(bar) = closure[1..].find('|') else {
        return closure.to_string();
    };
    let params_end = bar + 2;
    let params = &closure[..params_end];
    let mut body = closure[params_end..].trim();
    // Accept a single-statement block wrapper: `{ $x += 1 }`.
    if body.starts_with('{') && body.ends_with('}') && body.len() >= 2 {
        body = body[1..body.len() - 1].trim();
        body = body.strip_suffix(';').unwrap_or(body).trim();
    }
    if body.contains(';') {
        return closure.to_string();
    }
    let Some(rest) = body.strip_prefix('$') else {
        return closure.to_string();
    };
    let ident_len = rest
        .bytes()
        .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_')
        .count();
    if ident_len == 0 {
        return closure.to_string();
    }
    let (ident, tail) = rest.split_at(ident_len);
    let tail = tail.trim_start();
    let Some(op) = ["+=", "-=", "*=", "/=", "%="]
        .into_iter()
        .find(|o| tail.starts_with(o))
    else {
        return closure.to_string();
    };
    let rhs = tail[op.len()..].trim();
    if rhs.is_empty() {
        return closure.to_string();
    }
    // No parens around `rhs`: compound-assignment is Rust's lowest precedence, so `*__v *= a + b`
    // already binds as `*__v *= (a + b)` — wrapping would only trip `unused_parens`.
    format!("{params} ${ident}.update(|__v| *__v {op} {rhs})")
}

/// Replaces every `$ident` in `s` with `ident.get()` — a reactive read, for `[view]` interpolation where a signal reference is a value read.
pub(super) fn substitute_reads(s: &str) -> String {
    substitute_dollar(s, true)
}

/// Replaces every `$ident` in `s` with the bare `ident` (the signal handle), for closure bodies where `$count.update(…)` means the handle and `$` only marks it for cloning.
pub(super) fn substitute_handles(s: &str) -> String {
    substitute_dollar(s, false)
}

/// Rewrites each `$ident` to `ident` (plus `.get()` when `read`). Only an ASCII `$` followed by an identifier start counts as a marker; everything else is copied through unchanged.
fn substitute_dollar(s: &str, read: bool) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$'
            && bytes
                .get(i + 1)
                .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_')
        {
            let start = i + 1;
            let mut j = start;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            out.push_str(&s[start..j]);
            if read {
                out.push_str(".get()");
            }
            i = j;
        } else {
            let ch = s[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

/// Collects the identifier of every `$ident` signal reference in `s`, used to clone signals captured by a closure.
pub(super) fn signal_idents(s: &str) -> Vec<String> {
    let bytes = s.as_bytes();
    let mut idents = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$'
            && bytes
                .get(i + 1)
                .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_')
        {
            let start = i + 1;
            let mut j = start;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            idents.push(s[start..j].to_string());
            i = j;
        } else {
            i += 1;
        }
    }
    idents
}

/// The distinct identifiers a `move` closure must clone so its captures stay independent of the outer
/// bindings: every `$name` signal referenced across `snippets` (raw, still carrying `$`), deduped,
/// followed by any `loop_variables` a snippet uses (also deduped against the signals). Pass `&[]` for
/// `loop_variables` at a call site with no loop scope (e.g. the free-standing [`wrap_signal_clones`]);
/// the three clone emitters ([`wrap_signal_clones`], `clone_bindings`, `opacity_closure`) all format
/// this list for their own context (block wrapper / standalone statements / inline prefix).
pub(super) fn captured_idents(snippets: &[&str], loop_variables: &[String]) -> Vec<String> {
    let mut idents: Vec<String> = Vec::new();
    for s in snippets {
        for id in signal_idents(s) {
            if !idents.contains(&id) {
                idents.push(id);
            }
        }
    }
    for var in loop_variables {
        if snippets.iter().any(|s| contains_ident(s, var)) && !idents.contains(var) {
            idents.push(var.clone());
        }
    }
    idents
}

/// Wraps a `move` closure literal in a block that clones every `$name` signal referenced (raw, still carrying `$`) across `raw_values` first, generalized for `color_expr` callers, whose reads (e.g. `accent.get()`) are embedded inside an already-built closure string rather than assembled inline. A no-op when none of `raw_values` reference a signal, so a purely static/theme color emits the closure unchanged.
pub(super) fn wrap_signal_clones(raw_values: &[&str], closure_expr: String) -> String {
    // No loop scope is available here (free function), so loop variables are captured by move as before.
    let idents = captured_idents(raw_values, &[]);
    if idents.is_empty() {
        return closure_expr;
    }
    let prefix: String = idents
        .iter()
        .map(|s| format!("let {s} = {s}.clone(); "))
        .collect();
    format!("{{ {prefix}{closure_expr} }}")
}

/// Every raw source snippet a subtree contains, still carrying its `$` sigils — attribute values, text content, control-flow conditions and verbatim `let`s.
///
/// Collected so a `move` closure wrapping that subtree ([`clone_block_multiline`]) can clone the signals it will reference instead of moving them out of the surrounding view. Every emitter that puts view markup inside a `move` closure needs this: a reactive `if`/`for` branch and a `lazy` block alike.
pub(super) fn subtree_snippets(nodes: &[ViewNode]) -> Vec<String> {
    let mut out = Vec::new();
    collect_snippets(nodes, &mut out);
    out
}

fn collect_snippets(nodes: &[ViewNode], out: &mut Vec<String>) {
    for node in nodes {
        match node {
            ViewNode::Element(el) => {
                if let Some(content) = &el.content {
                    out.push(content.clone());
                }
                if let Some(params) = &el.leading_params {
                    out.push(params.clone());
                }
                for attr in &el.attributes {
                    out.push(attr.value.clone());
                }
                collect_snippets(&el.children, out);
            }
            ViewNode::IfBlock(block) => {
                out.push(block.condition.clone());
                collect_snippets(&block.then_branch, out);
                if let Some(else_branch) = &block.else_branch {
                    collect_snippets(else_branch, out);
                }
            }
            ViewNode::ForBlock(block) => {
                out.push(block.iterable.clone());
                if let Some(key) = &block.key_expr {
                    out.push(key.clone());
                }
                if let Some(gap) = &block.gap_expr {
                    out.push(gap.clone());
                }
                collect_snippets(&block.body, out);
            }
            ViewNode::MatchBlock(block) => {
                out.push(block.scrutinee.clone());
                if let Some(key) = &block.key_expr {
                    out.push(key.clone());
                }
                for arm in &block.arms {
                    collect_snippets(&arm.body, out);
                }
            }
            ViewNode::LetStmt(stmt) => out.push(stmt.source.clone()),
        }
    }
}

/// [`wrap_signal_clones`] for a closure whose body spans lines: the clones go on their own line above it, so the generated code stays readable and the source map keeps pointing at the right `.rsx` lines. A no-op when `idents` is empty, which keeps the common signal-free closure unwrapped.
pub(super) fn clone_block_multiline(idents: &[String], closure: String, pad: &str) -> String {
    if idents.is_empty() {
        return closure;
    }
    let mut out = format!("{pad}{{\n");
    for name in idents {
        let _ = writeln!(out, "{pad}    let {name} = {name}.clone();");
    }
    let _ = write!(out, "{closure}\n{pad}}}");
    out
}

/// Assembles a `&[(pos, color)]` gradient stops expression from the resolved `from`, `to`, and optional `mid`/`mid_pos` values.
pub(super) fn build_gradient_stops(
    from: &str,
    to: &str,
    mid: Option<&str>,
    mid_pos: f32,
) -> String {
    if let Some(m) = mid {
        format!(
            "&[(0.0, {from}), ({}, {m}), (1.0, {to})]",
            format_f32(mid_pos)
        )
    } else {
        format!("&[(0.0, {from}), (1.0, {to})]")
    }
}

/// Keys that contribute to a container's paint (`RectStyle`) rather than its layout. Used to pick which class props to merge into an element's paint attributes.
pub(super) fn is_paint_key(key: &str) -> bool {
    matches!(
        key,
        "fill"
            | "stroke"
            | "stroke_width"
            | "radius"
            | "opacity"
            | "gradient"
            | "from"
            | "to"
            | "mid"
            | "mid_pos"
            | "radial_radius"
    ) || key.starts_with("shadow")
}

/// Writes a styled widget's `transition:` prelude into its construction block: `compile_error!` lines for any diagnostics, then the hoisted `let __transition_N = motion::Animated::new(...)` handles. Both are emitted before the widget constructor so the animation handles are in scope for the closures that capture them, and the block runs once per instance (F7) so the animations persist across `view()` re-runs.
pub(super) fn emit_transition_prelude(
    code: &mut String,
    inner_pad: &str,
    errors: &[String],
    hoists: &[String],
) {
    for e in errors {
        let _ = writeln!(code, "{inner_pad}compile_error!({});", rust_str(e));
    }
    for h in hoists {
        let _ = writeln!(code, "{inner_pad}{h}");
    }
}

/// Whether any paint attribute is present, so a plain `col`/`row` must upgrade to a `StyledContainer`.
pub(super) fn has_paint(pattrs: &[Attr]) -> bool {
    pattrs.iter().any(|a| {
        matches!(
            a.key.as_str(),
            "fill" | "stroke" | "radius" | "opacity" | "gradient"
        ) || a.key.starts_with("shadow")
    })
}

/// Builds a `RectStyle { … }` or shorthand expression from the resolved fill, stroke, shadow, and radius values. Mirrors the branching logic shared by `emit_box` and `emit_canvas_rect`.
pub(super) fn build_rect_style(
    gradient: Option<String>,
    solid_fill: Option<String>,
    stroke: Option<String>,
    stroke_width: f32,
    shadow: Option<String>,
    radius: &str,
) -> String {
    if shadow.is_some() || stroke.is_some() || gradient.is_some() {
        let fill_s = gradient
            .map(|g| format!("Some({g})"))
            .or_else(|| solid_fill.map(|f| format!("Some(Paint::Solid({f}))")))
            .unwrap_or_else(|| "None".to_string());
        let stroke_s = stroke
            .map(|s| format!("Some(Stroke::new({s}, {}))", format_f32(stroke_width)))
            .unwrap_or_else(|| "None".to_string());
        let shadow_s = shadow.unwrap_or_else(|| "None".to_string());
        format!(
            "RectStyle {{ fill: {fill_s}, stroke: {stroke_s}, shadow: {shadow_s}, radius: {radius} }}"
        )
    } else {
        match solid_fill {
            Some(f) => format!("RectStyle::default().with_fill({f}).with_radius({radius})"),
            None => "RectStyle::default()".to_string(),
        }
    }
}

/// Binds canvas closure params (`w, h`) to fields of the `Rect` argument.
pub(super) fn canvas_param_bindings(params: &str, pad: &str) -> String {
    let mut out = String::new();
    let names: Vec<&str> = params
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    // Convention: first param is width, second is height.
    if let Some(w) = names.first() {
        let _ = writeln!(out, "{pad}    let {w} = __rect.width;");
    }
    if let Some(h) = names.get(1) {
        let _ = writeln!(out, "{pad}    let {h} = __rect.height;");
    }
    out
}
