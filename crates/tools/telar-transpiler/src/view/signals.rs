//! Free helper functions shared across the view emitters: `$signal` substitution, interpolation parsing, and the paint/rect-style/gradient builders.

use std::fmt::Write;

use telar_parser::{Attr, ViewNode};

use crate::lexer::contains_ident;
use crate::theme_access::{ThemeAccess, TokensRead};

use super::{ViewGen, expr_marker};

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

/// Renders a Rust string literal, escaping quotes, backslashes, and control chars. Newlines/tabs are escaped (not emitted raw) so a decoded newline in `.rsx` content stays a single line in the generated source — [`crate::view::resolve_source_map`] splits the body on `\n`, so a raw newline would desync it.
pub(crate) fn rust_str(s: &str) -> String {
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
    // The span covers the closure, not the markup's delimiting parens: a diagnostic should underline `|| toggle()`, not `(|| toggle())`.
    let text = attr.value.text();
    let trimmed = text.trim_start();
    let (trimmed, delimiters) = match super::redundant_parens(trimmed) {
        Some(inner) => (inner, 1),
        None => (trimmed, 0),
    };
    if !trimmed.starts_with('|') {
        return String::new();
    }
    let lead = text.len() - text.trim_start().len() + delimiters;
    expr_marker(attr.value_start + lead, trimmed.trim().len())
}

/// The parser strips `on_press:` leaving `|| expr` or `|ev| expr`. Ensure the value is a closure; wrap bare expressions in a zero-arg closure. Then desugar a lone compound-assignment on a signal.
pub(super) fn normalize_closure(value: &str) -> String {
    // The markup's delimiting parens are not part of the closure: `on_press:(|| f())` and `|| f()` are the same.
    let v = value.trim();
    let v = super::redundant_parens(v).unwrap_or(v);
    let closure = if telar_parser::starts_closure(v) {
        // The emitter prefixes `move` itself, so the two spellings `is_closure` accepts have to arrive here as one: keeping the author's `move` produced `move move ||`, and wrapping it as a body produced `move || { move || … }` — a closure returning a closure, which compiles and never runs.
        v.strip_prefix("move")
            .map_or(v, str::trim_start)
            .to_string()
    } else {
        format!("|| {{ {v} }}")
    };
    rewrite_compound_assign(&closure)
}

/// Sugar: a closure whose body is a single compound assignment on a signal (`|| $count += 1`, and the single-statement block form `|| { $count += 1 }`) is rewritten to `|| $count.update(|__v| *__v += (1))`. The `$` is kept so signal-cloning and [`substitute_handles`] still see the handle. Anything more complex (multiple statements, no compound operator, not a signal target) is returned unchanged.
fn rewrite_compound_assign(closure: &str) -> String {
    // `closure` starts with `|` (guaranteed by `normalize_closure`); split off its `||` / `|args|` head.
    let Some(bar) = closure[1..].find('|') else {
        return closure.to_string();
    };
    let params_end = bar + 2;
    let params = &closure[..params_end];
    let mut body = closure[params_end..].trim();
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
    // Compound assignment is Rust's lowest precedence, so `*__v *= a + b` already binds correctly and parens would only trip `unused_parens`.
    format!("{params} ${ident}.update(|__v| *__v {op} {rhs})")
}

/// Replaces every `$ident` in `s` with `ident.get()` — a reactive read, for `[view]` interpolation where a signal reference is a value read.
pub(crate) fn substitute_reads(s: &str, theme: ThemeAccess) -> String {
    substitute_dollar(s, true, theme)
}

/// Replaces every `$ident` in `s` with the bare `ident` (the signal handle), for closure bodies where `$count.update(…)` means the handle and `$` only marks it for cloning.
///
/// `$theme` is the exception, because a theme handle has no second use: reading it is the only thing anyone can do with one, so `$theme.primary` means the same read wherever it is written.
pub(super) fn substitute_handles(s: &str, theme: ThemeAccess) -> String {
    substitute_dollar(s, false, theme)
}

/// Rewrites each `$ident` to `ident` (plus `.get()` when `read`), each `$props.field` to that field's binding, and each library `$theme` read to its token; everything else is copied through unchanged.
fn substitute_dollar(s: &str, read: bool, theme: ThemeAccess) -> String {
    let mut out = String::with_capacity(s.len());
    let mut copied = 0;
    for (marker, end) in dollar_spans(s) {
        let ident = &s[marker + 1..end];
        out.push_str(&s[copied..marker]);
        if let Some(field) = prop_read(ident, &s[end..]) {
            let _ = write!(out, "props.{field}");
            if read {
                out.push_str(".get()");
            }
            copied = end + 1 + field.len();
            continue;
        }
        if let Some(tokens) = tokens_read(ident, &s[end..], theme) {
            out.push_str(&tokens.emit(""));
            copied = end + tokens.consumed();
            continue;
        }
        out.push_str(ident);
        if read || ident == "theme" {
            out.push_str(".get()");
        }
        copied = end;
    }
    out.push_str(&s[copied..]);
    out
}

/// [`substitute_reads`] that also records where each identifier came from. The expression around it is rewritten — the `$` goes, a `.get()` arrives — but the identifier itself is copied byte for byte, and that is the part a cursor lands on. `base` is the source byte offset of `s`. A library's token read records its token's name instead, the part that names something.
pub(super) fn substitute_reads_spanned(s: &str, base: usize, theme: ThemeAccess) -> String {
    let mut out = String::with_capacity(s.len());
    let mut copied = 0;
    for (marker, end) in dollar_spans(s) {
        let ident = &s[marker + 1..end];
        out.push_str(&s[copied..marker]);
        if let Some(field) = prop_read(ident, &s[end..]) {
            out.push_str(&expr_marker(base + marker + 1, ident.len()));
            let _ = write!(out, "props.{field}.get()");
            copied = end + 1 + field.len();
            continue;
        }
        if let Some(tokens) = tokens_read(ident, &s[end..], theme) {
            let mark = match tokens {
                TokensRead::Token { name, .. } => expr_marker(base + end + 1, name.len()),
                _ => String::new(),
            };
            out.push_str(&tokens.emit(&mark));
            copied = end + tokens.consumed();
            continue;
        }
        out.push_str(&expr_marker(base + marker + 1, ident.len()));
        out.push_str(ident);
        out.push_str(".get()");
        copied = end;
    }
    out.push_str(&s[copied..]);
    out
}

/// The library read a `$ident` followed by `after` stands for, or `None` when it is not one.
fn tokens_read<'a>(ident: &str, after: &'a str, theme: ThemeAccess) -> Option<TokensRead<'a>> {
    (theme == ThemeAccess::Tokens && ident == "theme").then(|| TokensRead::parse(after))
}

/// The field a `$props` marker followed by `after` reads, or `None` when it is not a `$props.field`. `$` binds to the identifier everywhere else (`$item.name` reads `item` and then takes `name`), but `props` is not reactive: its fields are, so the read reaches through it to the field, the way a library's `$theme.primary` reaches through to its token.
fn prop_read<'a>(ident: &str, after: &'a str) -> Option<&'a str> {
    if ident != "props" {
        return None;
    }
    let tail = after.strip_prefix('.')?;
    let len = tail
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(tail.len());
    let field = &tail[..len];
    field
        .starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        .then_some(field)
}

/// Collects the identifier of every `$ident` signal reference in `s`, used to clone signals captured by a closure. A library's `$theme` is none: it reads a function, not a binding. Nor is a `$props.field`: it is a place in `props`, which the capture of `props` places copies in on its own.
pub(super) fn signal_idents(s: &str, theme: ThemeAccess) -> Vec<String> {
    dollar_spans(s)
        .filter(|&(marker, end)| prop_read(&s[marker + 1..end], &s[end..]).is_none())
        .map(|(marker, end)| s[marker + 1..end].to_string())
        .filter(|ident| !(theme == ThemeAccess::Tokens && ident == "theme"))
        .collect()
}

/// Every `$ident` marker in `s` as a `(dollar, end)` byte span. Only an ASCII `$` followed by an identifier start counts as a marker; a byte-wise walk cannot split a multi-byte char, since no continuation byte is ASCII.
fn dollar_spans(s: &str) -> impl Iterator<Item = (usize, usize)> {
    let bytes = s.as_bytes();
    let mut i = 0;
    std::iter::from_fn(move || {
        while i < bytes.len() {
            if bytes[i] == b'$'
                && bytes
                    .get(i + 1)
                    .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_')
            {
                let marker = i;
                i += 1;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                return Some((marker, i));
            }
            i += 1;
        }
        None
    })
}

/// The distinct names a `move` closure reading `snippets` clones in: every `$name` signal they reference, then every one of `loop_variables` and `locals` they use, deduped. A name a snippet's own scope binds is the closure's, not a capture.
///
/// A binding that is not `Clone` cannot be copied into a closure that needs its own, and saying so at the clone is the honest place: the alternative is a move-out-of-`Fn` error pointing at generated code the author never wrote.
fn captured_from(
    snippets: &[(&str, &[String])],
    loop_variables: &[String],
    locals: &[String],
    theme: ThemeAccess,
) -> Vec<String> {
    let mut idents: Vec<String> = Vec::new();
    for (s, shadowed) in snippets {
        for id in signal_idents(s, theme) {
            if !shadowed.contains(&id) && !idents.contains(&id) {
                idents.push(id);
            }
        }
    }
    let named: Vec<Option<Vec<String>>> = snippets
        .iter()
        .map(|(s, _)| crate::rust::free_idents(&substitute_reads(s, theme)))
        .collect();
    for var in loop_variables.iter().chain(locals) {
        let used = snippets.iter().zip(&named).any(|((s, shadowed), free)| {
            !shadowed.contains(var)
                && match free {
                    // The expression parsed, so `seat(&desk, id).x` is known not to use a binding called `x` — cloning one would be a compile error in generated code.
                    Some(free) => free.contains(var),
                    // It did not parse (macro tokens, a half-written value). A missed capture is a move out of an `Fn`, which is worse than a clone nobody needed.
                    None => contains_ident(s, var),
                }
        });
        if used && !idents.contains(var) {
            idents.push(var.clone());
        }
    }
    idents
}

/// Snippets read with nothing bound between them and the closure reading them.
pub(super) fn unscoped<'a>(raw_values: &[&'a str]) -> Vec<(&'a str, &'a [String])> {
    raw_values.iter().map(|s| (*s, &[][..])).collect()
}

/// The `let` that gives a closure its own `name`, marked so the source map knows the binding shadows the one it copies.
///
/// `name.clone()`, not [`copy_of`]: a `move` closure the view emits must be `'static`, so a binding that is a reference has to become the value it points at, which is what the method call's auto-deref does.
pub(super) fn clone_statement(name: &str) -> String {
    format!("let {}{name} = {name}.clone();", super::shadow_marker(name))
}

/// A copy of `expr` handed on by value, of exactly its type: `Clone::clone(&x)`, since what takes it declares that type, where `x.clone()` would turn a binding that is a reference into a clone of what it points at.
pub(super) fn copy_of(expr: &str) -> String {
    format!("::core::clone::Clone::clone(&{expr})")
}

/// `closure` behind one `let x = x.clone();` per name and then `copies`, whole `let` statements, inside a block so the whole thing is still an expression; bare when there is nothing to take, since a block holding only the closure is the `unused_braces` rustc warns of in generated code.
fn capture_block(idents: &[String], copies: &[String], closure: String) -> String {
    if idents.is_empty() && copies.is_empty() {
        return closure;
    }
    let prefix: String = idents
        .iter()
        .map(|name| format!("{} ", clone_statement(name)))
        .chain(copies.iter().map(|copy| format!("{copy} ")))
        .collect();
    format!("{{ {prefix}{closure} }}")
}

/// [`capture_block`] for a closure whose body spans lines: the clones go on lines of their own above it at `pad`, so the generated code stays readable and the source map keeps pointing at the right `.rsx` lines.
fn capture_block_multiline(
    idents: &[String],
    copies: &[String],
    closure: String,
    pad: &str,
) -> String {
    if idents.is_empty() && copies.is_empty() {
        return closure;
    }
    let mut out = format!("{pad}{{\n");
    for name in idents {
        let _ = writeln!(out, "{pad}    {}", clone_statement(name));
    }
    for copy in copies {
        let _ = writeln!(out, "{pad}    {copy}");
    }
    let _ = write!(out, "{closure}\n{pad}}}");
    out
}

/// Every source marker in `code`: where it is written, where it ends, and how many bytes after it it covers.
fn source_markers(code: &str) -> Vec<(usize, usize, usize)> {
    let mut markers = Vec::new();
    let mut from = 0;
    while let Some(found) = code[from..].find(super::SRC_EXPR_OPEN) {
        let at = from + found;
        let body = at + super::SRC_EXPR_OPEN.len();
        let Some(close) = code[body..].find(super::SRC_EXPR_CLOSE) else {
            break;
        };
        let end = body + close + super::SRC_EXPR_CLOSE.len();
        if let Some(len) = code[body..body + close]
            .split_once(':')
            .and_then(|(_, len)| len.parse().ok())
        {
            markers.push((at, end, len));
        }
        from = end;
    }
    markers
}

/// `code` with each of `places` read through a copy of its own, pushing the `let` that makes each copy onto `copies` once, so a `move` closure takes the copy and leaves the place to every other reader. `marker` is the source marker for the place at a byte offset.
///
/// A marker already in `code` covering text within a place moves onto its copy, where that text still is; one covering more than the place is dropped, since the text it spans changes.
pub(super) fn copy_places(
    code: &str,
    places: &[(usize, usize)],
    marker: impl Fn(usize, &str) -> String,
    copies: &mut Vec<String>,
) -> String {
    let markers = source_markers(code);
    let mut dropped: Vec<(usize, usize)> = Vec::new();
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    for &(start, end) in places {
        let place = &code[start..end];
        let binding = format!("__{}", crate::rust::place_text(place).replace('.', "_"));
        let mut copied = place.to_string();
        let mut inner: Vec<(usize, &str)> = Vec::new();
        for &(at, marker_end, len) in &markers {
            let covered = (marker_end, marker_end + len);
            if covered.1 <= start || covered.0 >= end {
                continue;
            }
            dropped.push((at, marker_end));
            if covered.0 >= start && covered.1 <= end {
                inner.push((covered.0 - start, &code[at..marker_end]));
            }
        }
        for (offset, text) in inner.into_iter().rev() {
            copied.insert_str(offset, text);
        }
        let copy = format!("let {binding} = {}{copied}.clone();", marker(start, place));
        if !copies.contains(&copy) {
            copies.push(copy);
        }
        edits.push((start, end, binding));
    }
    edits.extend(
        dropped
            .into_iter()
            .map(|(at, end)| (at, end, String::new())),
    );
    edits.sort_by_key(|&(at, _, _)| at);
    edits.dedup_by_key(|edit| edit.0);
    let mut rewritten = String::with_capacity(code.len());
    let mut done = 0;
    for (at, end, with) in edits {
        rewritten.push_str(&code[done..at]);
        rewritten.push_str(&with);
        done = end;
    }
    rewritten.push_str(&code[done..]);
    rewritten
}

impl ViewGen<'_> {
    /// The one capture rule for a `move` closure the view emits, built from the source `raw_values`: every `$signal` and loop variable they name is cloned in, and every `[logic]` binding and `props` place the closure reads is copied in where [`Self::must_copy_capture`] says moving it would take it from another reader, and moved where nothing else needs it.
    pub(super) fn owning(&self, raw_values: &[&str], closure: &str) -> String {
        let snippets = unscoped(raw_values);
        let (idents, copies, closure) = self.captures(&snippets, closure);
        capture_block(&idents, &copies, closure)
    }

    /// [`Self::owning`] for a closure that builds a subtree, from the subtree's scoped `snippets`, with its clones on lines of their own at `pad`.
    pub(super) fn owning_subtree(
        &self,
        snippets: &[ScopedSnippet],
        closure: String,
        pad: &str,
    ) -> String {
        let scoped: Vec<(&str, &[String])> = snippets
            .iter()
            .map(|s| (s.text.as_str(), s.shadowed.as_slice()))
            .collect();
        let (idents, copies, closure) = self.captures(&scoped, &closure);
        capture_block_multiline(&idents, &copies, closure, pad)
    }

    /// What a `move` closure reading `snippets` takes in: the names it clones, the `props` places it copies, and the closure reading those copies.
    fn captures(
        &self,
        snippets: &[(&str, &[String])],
        closure: &str,
    ) -> (Vec<String>, Vec<String>, String) {
        let places: Vec<(usize, usize)> = crate::rust::field_places(closure, "props")
            .into_iter()
            .filter(|&(start, end)| {
                self.must_copy_capture(
                    self.reads
                        .place(&crate::rust::place_text(&closure[start..end])),
                )
            })
            .collect();
        let mut copies = Vec::new();
        let closure = copy_places(closure, &places, |_, _| String::new(), &mut copies);
        (self.captured_names(snippets), copies, closure)
    }

    /// `written`, the emitted form of `expr`, taken by value when `expr` names a `[logic]` binding, a binding the view makes (a loop variable, an `if let` or arm binding, which [`ViewReads`](super::ViewReads) does not count, so each is one reader of its own), or a `props` place: a [`copy_of`] it where [`Self::must_copy_capture`] says moving it would take it from another reader or from the next run of a rebuilding region, itself where it can be moved. `None` for an expression that names no binding to take.
    pub(super) fn by_value(&self, expr: &str, written: String) -> Option<String> {
        let readers = if self.is_local(expr) {
            self.reads.local(expr)
        } else if self.loop_variables.iter().any(|bound| bound == expr) {
            1
        } else if crate::rust::field_places(expr, "props") == [(0, expr.len())] {
            self.reads.place(&crate::rust::place_text(expr))
        } else {
            return None;
        };
        Some(match self.must_copy_capture(readers) {
            true => copy_of(&written),
            false => written,
        })
    }

    /// The bindings a `move` closure reading `snippets` clones in: every `$signal` and loop variable they name, and every `[logic]` binding moving would take from another reader. `props` is not one: its places are copied one by one.
    pub(super) fn captured_names(&self, snippets: &[(&str, &[String])]) -> Vec<String> {
        let locals: Vec<String> = self
            .locals
            .iter()
            .filter(|local| *local != "props" && self.must_copy_capture(self.reads.local(local)))
            .cloned()
            .collect();
        captured_from(snippets, &self.loop_variables, &locals, self.theme_access)
    }
}
/// One raw source snippet of a subtree, with the names bound between it and the top of that subtree — a view `let` above it, an enclosing loop or arm pattern, the wrapping closure's own parameters.
///
/// Those names belong to the closure being wrapped, so an identifier a snippet shares with one of them is a reference to what the closure itself declares and not a capture to clone in from outside.
pub(super) struct ScopedSnippet {
    text: String,
    shadowed: Vec<String>,
}

impl ScopedSnippet {
    fn new(text: String, scope: &[String]) -> Self {
        Self {
            text,
            shadowed: scope.to_vec(),
        }
    }
}

/// Every raw source snippet a subtree contains, still carrying its `$` sigils — attribute values, text content, control-flow conditions and verbatim `let`s.
///
/// Collected so a `move` closure wrapping that subtree ([`ViewGen::owning_subtree`]) takes what it reads without moving it out of the surrounding view, and so [`ViewReads`](super::ViewReads) can count who reads what. Every emitter that puts view markup inside a `move` closure needs this: a reactive `if`/`for` branch, a `lazy` block and a component's `Children` recipe alike.
pub(super) fn subtree_snippets(nodes: &[ViewNode]) -> Vec<String> {
    scoped_snippets(nodes, &[])
        .into_iter()
        .map(|s| s.text)
        .collect()
}

/// The same walk, keeping what each snippet is read against. `bound` seeds the scope with the names the wrapping closure's parameters introduce — a `for`'s pattern, a `match` arm's — which sit inside it exactly as a view `let` does.
pub(super) fn scoped_snippets(nodes: &[ViewNode], bound: &[String]) -> Vec<ScopedSnippet> {
    let mut out = Vec::new();
    collect_snippets(nodes, &mut bound.to_vec(), &mut out);
    out
}

fn collect_snippets(nodes: &[ViewNode], scope: &mut Vec<String>, out: &mut Vec<ScopedSnippet>) {
    for node in nodes {
        let depth = scope.len();
        match node {
            ViewNode::Element(el) => {
                // Only the `{…}` holes are source. Pushing the whole string made `syn` lex prose, where an `I"` or a stray exponent is a hard lexer error inside a proc macro rather than a `Result`.
                if let Some(content) = &el.content {
                    for segment in parse_interpolation(content) {
                        if let Segment::Expr { text, .. } = segment {
                            out.push(ScopedSnippet::new(text, scope));
                        }
                    }
                }
                for attr in &el.attributes {
                    // A quoted value is data, handed through as a string literal with no `$` substitution. Scanning it made a doc string mentioning `$x` clone a binding called `x` that the file never had.
                    if !attr.value.is_quoted() {
                        out.push(ScopedSnippet::new(attr.value.text().to_string(), scope));
                    }
                }
                // Each state of a box is a paint closure of its own laid over the base paint, so the base paint has a reader more per state.
                for _ in 0..box_states(el) {
                    for attr in el
                        .attributes
                        .iter()
                        .filter(|a| is_paint_key(&a.key) || a.key == "surface")
                    {
                        out.push(ScopedSnippet::new(attr.value.text().to_string(), scope));
                    }
                }
                // A placeholder's positional expression parses as a flag, whose key is the source a rebuilding closure around it has to clone in.
                if el.tag == "children" {
                    for operand in el.attributes.iter().filter(|a| a.value.is_flag()) {
                        out.push(ScopedSnippet::new(operand.key.clone(), scope));
                    }
                }
                collect_snippets(&el.children, scope, out);
            }
            ViewNode::IfBlock(block) => {
                out.push(ScopedSnippet::new(block.condition.clone(), scope));
                collect_snippets(&block.then_branch, scope, out);
                // The branches are separate blocks: a `let` in one is not in scope in the other.
                scope.truncate(depth);
                if let Some(else_branch) = &block.else_branch {
                    collect_snippets(else_branch, scope, out);
                }
            }
            ViewNode::ForBlock(block) => {
                out.push(ScopedSnippet::new(block.iterable.clone(), scope));
                let pattern = pattern_idents(&block.pattern);
                if let Some(key) = &block.key_expr {
                    let keyed: Vec<String> = scope.iter().chain(&pattern).cloned().collect();
                    out.push(ScopedSnippet::new(key.clone(), &keyed));
                }
                // The gap is emitted as an argument to the list constructor, outside the item closure the pattern belongs to.
                if let Some(gap) = &block.gap_expr {
                    out.push(ScopedSnippet::new(gap.clone(), scope));
                }
                scope.extend(pattern);
                collect_snippets(&block.body, scope, out);
            }
            ViewNode::MatchBlock(block) => {
                out.push(ScopedSnippet::new(block.scrutinee.clone(), scope));
                if let Some(key) = &block.key_expr {
                    let mut keyed = scope.clone();
                    keyed.extend(block.binding.clone());
                    out.push(ScopedSnippet::new(key.clone(), &keyed));
                }
                for arm in &block.arms {
                    scope.extend(pattern_idents(&arm.pattern));
                    collect_snippets(&arm.body, scope, out);
                    scope.truncate(depth);
                }
            }
            ViewNode::LetStmt(stmt) => {
                out.push(ScopedSnippet::new(stmt.source.clone(), scope));
                scope.extend(let_bound_names(&stmt.source));
            }
            ViewNode::Comment(_) => {}
        }
        // A view `let` stays in scope for the siblings after it; every other node closes the block it opened.
        if !matches!(node, ViewNode::LetStmt(_)) {
            scope.truncate(depth);
        }
    }
}

/// How many state paints a `box`, `row`, `col` or `grid` swaps its base paint for.
fn box_states(el: &telar_parser::Element) -> usize {
    match el.tag.as_str() {
        "box" | "row" | "col" | "grid" => el
            .attributes
            .iter()
            .filter(|a| {
                matches!(
                    a.key.as_str(),
                    "hover_style" | "active_style" | "disabled_style"
                )
            })
            .count(),
        _ => 0,
    }
}

/// The names a view `let` binds, falling back to a scan of its pattern text when the statement does not parse — half-written mid-keystroke, or holding macro tokens.
fn let_bound_names(source: &str) -> Vec<String> {
    crate::rust::let_bindings(source).unwrap_or_else(|| {
        source
            .trim()
            .strip_prefix("let ")
            .and_then(|rest| rest.split('=').next())
            .map(pattern_idents)
            .unwrap_or_default()
    })
}

/// Assembles a `&[(pos, color)]` gradient stops expression from the resolved `from`, `to`, and optional `mid`/`mid_pos` values. Keys that contribute to a container's paint (`RectStyle`) rather than its layout. Used to pick which class props to merge into an element's paint attributes.
pub(crate) fn is_paint_key(key: &str) -> bool {
    matches!(
        key,
        "fill" | "stroke" | "stroke_width" | "radius" | "opacity" | "backdrop_blur"
    ) || key.starts_with("shadow")
        || is_side_key(key)
        || is_corner_key(key)
}

/// A name for one side of the border — `stroke_bottom`, `stroke_x`, `stroke_end`.
pub(super) fn is_side_key(key: &str) -> bool {
    key.strip_prefix("stroke_")
        .is_some_and(|s| crate::edges::side_target(s).is_some())
}

/// A name for one corner, or one edge's pair of them — `radius_top_left`, `radius_top`, `radius_start`.
pub(super) fn is_corner_key(key: &str) -> bool {
    key.strip_prefix("radius_")
        .is_some_and(|s| crate::edges::corner_target(s).is_some())
}

/// Writes a styled widget's `transition(…)` prelude into its construction block: `compile_error!` lines for any diagnostics, then the hoisted `let __transition_N = motion::Animated::new(...)` handles. Both are emitted before the widget constructor so the animation handles are in scope for the closures that capture them, and the block runs once per instance (F7) so the animations persist across `view()` re-runs.
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
            "fill" | "stroke" | "radius" | "opacity" | "blend" | "backdrop_blur"
        ) || a.key.starts_with("shadow")
            // A corner counts the way `radius` does; a side does not, the way `stroke_width` does not.
            || is_corner_key(&a.key)
    })
}

/// Builds a `RectStyle { … }` or shorthand expression from the resolved fill, stroke, shadow, and radius values. Mirrors the branching logic shared by `emit_box` and `emit_canvas_rect`.
pub(super) fn build_rect_style(
    gradient: Option<String>,
    solid_fill: Option<String>,
    stroke: Option<String>,
    border_widths: String,
    shadow: Option<String>,
    radius: &str,
) -> String {
    if shadow.is_some() || stroke.is_some() || gradient.is_some() {
        let fill_s = gradient
            .map(|g| format!("Some({g})"))
            .or_else(|| solid_fill.map(|f| format!("Some(Paint::Solid({f}))")))
            .unwrap_or_else(|| "None".to_string());
        let border_s = match stroke {
            Some(s) => {
                format!("Some(Border {{ paint: Paint::Solid({s}), widths: {border_widths} }})")
            }
            None => "None".to_string(),
        };
        let shadow_s = shadow.unwrap_or_else(|| "None".to_string());
        format!(
            "RectStyle {{ fill: {fill_s}, border: {border_s}, shadow: {shadow_s}, radius: {radius} }}"
        )
    } else {
        match solid_fill {
            Some(f) => format!("RectStyle::default().with_fill({f}).with_radius({radius})"),
            None => "RectStyle::default()".to_string(),
        }
    }
}
