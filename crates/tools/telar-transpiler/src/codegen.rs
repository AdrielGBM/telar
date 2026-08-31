//! The codegen engine: turns a parsed [`RsxDocument`] into compilable Rust source, wiring the `[logic]`, `[style]`, `[view]`, and `[preview]` zones together with a per-line source map.

use std::path::Path;

use telar_parser::{RsxDocument, ViewNode};

use crate::error::TranspileError;
use crate::naming::{
    contains_ident, literal_or_comment_end, preview_entries_const_name, replace_whole_word,
    to_pascal_case, to_snake_case,
};
use crate::signal_scan::{scan_locals, scan_signals};
use crate::source_map::ExprSpan;
use crate::style::generate_style_section;
use crate::view::ViewGen;

/// A parsed `Props` field: its name, its type, and any inline default expression (the `name: Type = expr`
/// sugar). Whether it is `Option<...>` is no longer anyone's business here — the builder's `some` attribute
/// answers that in the callee's own declaration.
struct ParsedField {
    name: String,
    ty: String,
    default: Option<String>,
    /// Attribute lines the author wrote above the field, carried through verbatim.
    attrs: String,
}

/// Finds the byte index of the top-level `=` that separates a field type from an inline default
/// expression (`name: Type = expr`), or `None`. Skips `=` inside angle brackets and the
/// `==`/`=>`/`<=`/`>=`/`!=` operators, and treats `->` as an arrow (not a generic close) so a type
/// like `Box<dyn Fn() -> T>` keeps correct bracket depth.
fn find_default_sep(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    let mut depth = 0i32;
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'<' => depth += 1,
            b'>' if i > 0 && b[i - 1] == b'-' => {}
            b'>' => depth = (depth - 1).max(0),
            b'=' if depth == 0 => {
                let prev = if i > 0 { b[i - 1] } else { 0 };
                let next = if i + 1 < b.len() { b[i + 1] } else { 0 };
                if !matches!(prev, b'=' | b'<' | b'>' | b'!') && !matches!(next, b'=' | b'>') {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Splits a struct body into field chunks on top-level commas, so a comma inside a default expression
/// (e.g. `= Color::rgba(1.0, 0.0, 0.0, 1.0)`) or a generic (`Vec<A, B>`) does not split a field.
///
/// Comments and string literals are skipped whole. A field's own doc comment is prose, and prose has commas in
/// it — counting those split the field away from its type and dropped it from the struct without a word, which
/// surfaced much later as a missing field at the first call site.
fn split_top_level_commas(body: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    let b = body.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if let Some(next) = literal_or_comment_end(b, i) {
            i = next;
            continue;
        }
        match c {
            b'(' | b'[' | b'{' | b'<' => depth += 1,
            b')' | b']' | b'}' => depth = (depth - 1).max(0),
            b'>' if i > 0 && b[i - 1] == b'-' => {}
            b'>' => depth = (depth - 1).max(0),
            b',' if depth == 0 => {
                chunks.push(body[start..i].to_string());
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    chunks.push(body[start..].to_string());
    chunks
}

/// Indices of the logic-zone lines that make up a top-level `use` statement, in source order.
///
/// Column 0 is the test, so a `use` inside a nested `fn` or block stays where its author put it. An unterminated
/// statement is left alone rather than guessed at, so a malformed import fails where it was written.
fn hoisted_use_lines(logic: &str) -> Vec<usize> {
    let lines: Vec<&str> = logic.lines().collect();
    let mut hoisted = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if lines[i].starts_with("use ") {
            let start = i;
            while i < lines.len() && !lines[i].trim_end().ends_with(';') {
                i += 1;
            }
            if i < lines.len() {
                hoisted.extend(start..=i);
            } else {
                i = start;
            }
        }
        i += 1;
    }
    hoisted
}

/// Extracts a `Props` field from a `[pub] name: Type[ = default]` chunk, skipping comment lines.
/// The author's `#[props(…)]` with their inline `= expr` folded in, since the derive reads one such
/// attribute per field and a prop carrying both otherwise loses the default.
fn merged_attrs(attrs: &str, default: Option<&str>) -> String {
    let Some(expr) = default else {
        return attrs.to_string();
    };
    attrs
        .lines()
        .map(
            |line| match line.contains("#[props(") && !line.contains("default") {
                true => format!(
                    "{}\n",
                    line.replacen("#[props(", &format!("#[props(default = {expr}, "), 1)
                ),
                false => format!("{line}\n"),
            },
        )
        .collect()
}

fn parse_field(chunk: &str) -> Option<ParsedField> {
    let attrs: String = chunk
        .lines()
        .filter(|l| l.trim_start().starts_with("#["))
        .map(|l| format!("    {}\n", l.trim()))
        .collect();
    let cleaned = chunk
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            !t.starts_with("//") && !t.starts_with("#[")
        })
        .collect::<Vec<_>>()
        .join(" ");
    let t = cleaned.trim();
    let t = t.strip_prefix("pub ").unwrap_or(t).trim_start();
    let colon = t.find(':')?;
    let name = t[..colon].trim();
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    let rest = t[colon + 1..].trim_start();
    let (ty, default) = match find_default_sep(rest) {
        Some(i) => (rest[..i].trim(), Some(rest[i + 1..].trim().to_string())),
        None => (rest.trim(), None),
    };
    Some(ParsedField {
        name: name.to_string(),
        ty: ty.to_string(),
        default,
        attrs,
    })
}

/// Input to a single transpilation: the parsed document plus the desired component function name (typically derived from the source file stem).
pub(crate) struct TranspileInput<'a> {
    pub document: &'a RsxDocument,
    pub component_name: &'a str,
    /// Concrete theme type path (e.g. `SandboxTheme`). When set, the generated view binds `theme` as a `telar::Theme<Type>` handle, which `$theme.field` reads.
    pub theme_type: Option<&'a str>,
    /// Directory of the `.rsx` being transpiled, used to resolve static `svg`/`img` asset paths (`src:"path"`) for build-time baking. `None` when no filesystem anchor is available (e.g. some analyzer/test paths), in which case a static asset yields a `compile_error!`.
    pub base_dir: Option<&'a Path>,
}

/// The generated Rust source for one `.rsx` file.
pub struct TranspiledSource {
    pub rust_code: String,
    pub preview_names: Vec<String>,
    /// Per generated line (0-based), the 0-based `.rsx` line it originated from, or `None` for boilerplate and transpiler-injected lines. Lets the analyzer map rust-analyzer's diagnostics on the generated code back onto the `.rsx` source.
    pub source_map: Vec<Option<u32>>,
    /// Byte spans of verbatim `[view]` Rust expressions, mapping a `.rsx` source range to the generated Rust. The half of the map that makes a *column* mean something; persisted into the `.rs.map` beside [`Self::source_map`]. See [`ExprSpan`].
    pub expr_spans: Vec<ExprSpan>,
}

/// Parses `source` and generates Rust for `component_name`, resolving `[style]` colors through `theme_type`
/// when provided so theme switching at runtime takes effect. `base_dir` is the directory of the `.rsx` (its
/// parent), against which static `svg`/`img` asset paths are resolved and baked at build time.
///
/// **It takes no registry, and that is the point.** A call site used to need the callee's shape — which
/// props it declared, which were optional, which took a closure, whether it accepted children — so every
/// `.rsx` in the workspace had to be scanned before any one of them could be transpiled. Every component
/// takes the same two arguments now, and the props builder answers the rest in the callee's own type, so a
/// file transpiles knowing nothing but itself.
pub fn transpile_source(
    source: &str,
    component_name: &str,
    theme_type: Option<&str>,
    base_dir: Option<&Path>,
) -> Result<TranspiledSource, TranspileError> {
    let document = telar_parser::parse(source)?;
    transpile(TranspileInput {
        document: &document,
        component_name,
        theme_type,
        base_dir,
    })
}

/// Accumulates generated code together with a per-line origin map. Each completed line (terminated by `\n`) records the `.rsx` source line passed when its newline was appended, so callers tag a line by emitting its content and the closing newline with the same `src`.
#[derive(Default)]
struct Code {
    out: String,
    map: Vec<Option<u32>>,
}

impl Code {
    fn push(&mut self, text: &str, src: Option<u32>) {
        for ch in text.chars() {
            self.out.push(ch);
            if ch == '\n' {
                self.map.push(src);
            }
        }
    }
}

fn transpile(input: TranspileInput<'_>) -> Result<TranspiledSource, TranspileError> {
    let doc = input.document;
    let fn_name = to_snake_case(input.component_name);
    if fn_name.is_empty() {
        return Err(TranspileError::Codegen(
            "component name is empty or has no valid identifier characters".into(),
        ));
    }

    let (props_struct, props_origins, props_span) =
        extract_props_struct(&doc.logic.source, &fn_name);
    let (context_struct, context_span) = extract_context_struct(&doc.logic.source, &fn_name);
    let props_type = to_pascal_case(&fn_name) + "Props";
    // A view that declares none still has a props type, holding nothing. The call site writes
    // `XProps::props().build()` either way, which is what lets the emitter stop asking whether there is one.
    let props_struct = props_struct.or_else(|| {
        Some(format!(
            "#[derive(::telar::Props)]\npub struct {props_type} {{}}"
        ))
    });
    // Ascending, because the line-number reconstruction below restores them one after another.
    let mut lifted: Vec<(usize, usize)> =
        [props_span, context_span].into_iter().flatten().collect();
    lifted.sort();
    // The struct declarations move to module scope; what is left is the body, emitted byte for byte. The
    // author still writes the bare `Context` to build one, and an alias in the body is what lets them —
    // renaming it here instead would shift every column on the line, and `[logic]` diagnostics land on the
    // columns rustc gave them precisely because it is transpiled 1:1.
    let logic_lifted = without_line_spans(&doc.logic.source, &lifted);
    let logic_source = if lifted.is_empty() {
        &doc.logic.source
    } else {
        &logic_lifted
    };

    let signals = scan_signals(logic_source);

    let style_section = generate_style_section(&doc.style, input.theme_type);

    let mut view_gen = ViewGen::with_theme(&doc.style.classes, input.theme_type, input.base_dir)
        .with_locals(scan_locals(logic_source))
        .with_signals(signals.iter().map(|s| s.name.clone()).collect());
    let view_body = view_gen.generate_root(&doc.view.nodes);
    let uses_theme = view_gen.uses_theme();

    let logic = logic_source.trim_end().to_string();

    // A `children` placeholder anywhere in the view makes the component take a `Slots` argument.
    let has_slot = view_uses_slot(&doc.view.nodes);
    // A `Context` struct with nowhere to put children is not a compound component — it declares a type nobody
    // is handed, and taking a recipe the body never runs would only produce an unused argument.
    let ret = "Result<Box<dyn LayoutItem>, LayoutError>";
    // **Every component has the same signature, and that is what let the registry go.** A call site used to
    // need the callee's arity — does it take props, does it take children, does it want them built or
    // deferred — which meant a table describing every component in the workspace. Taking both always leaves
    // nothing to ask: a view that names no props gets an empty builder, and one that never places its
    // children simply never runs the recipe.
    let signature = format!("pub fn {fn_name}(props: {props_type}, children: Children) -> {ret}");

    // 0-based `.rsx` line of `logic_source` line 0, used to map generated lines back to the source.
    let logic_start0 = doc.logic.start_line.saturating_sub(1) as u32;
    // `logic_source` (with the lifted structs removed) line index -> its 0-based `.rsx` line. The spans go back
    // in ascending order, so each comparison is made against a line number the earlier ones have already
    // restored.
    let logic_line_src = |j: usize| -> u32 {
        let mut orig = j;
        for &(start, end) in &lifted {
            if orig >= start {
                orig += end - start + 1;
            }
        }
        logic_start0 + orig as u32
    };

    let mut code = Code::default();
    code.push(
        "// Generated by telar-transpiler — do not edit manually\n",
        None,
    );
    // Silence clippy for the whole generated module: this is machine-emitted code the consumer can't edit, so
    // lints like `clone_on_copy` (a loop var cloned into a closure) or `collapsible_if` are pure noise on
    // `cargo clippy`. Only clippy is suppressed — rustc errors/warnings still surface (and the analyzer maps
    // them back onto the `.rsx` source), so real mistakes in `[logic]`/`[view]` are unaffected.
    code.push("#![allow(clippy::all)]\n", None);
    // The clone emitter cannot tell a `&[T]` from an owned one, and cloning a reference is a no-op rustc
    // warns about. Allowed for the same reason clippy is: nobody edits this file, so a lint here reaches
    // no one who could act on it.
    code.push("#![allow(noop_method_call)]\n", None);
    code.push("#[allow(unused_imports)] use telar::*;\n", None);
    // The crate root's own items — an app's `theme` module, its constants — which is what a `[logic]` line
    // naming `core::theme::SandboxTheme` means. Deliberately not `use super::*` as well: a `.rsx` named after
    // anything in telar's prelude would make that name ambiguous in its own siblings, and a neighbour a file
    // wants is a neighbour it can name. A *component* is reached the same way — it lives at its own path and
    // the author imports it, which is the whole of the namespacing fix.
    code.push("#[allow(unused_imports)] use crate::*;\n", None);

    // The logic zone's own imports, lifted to module scope: `Props` and each `[preview]` are emitted as siblings
    // of the component function, so a `use` left in its body would be out of scope for exactly the declarations
    // most likely to name an imported type. Inside the body the two placements are equivalent.
    let hoisted_uses = hoisted_use_lines(logic_source);
    if !hoisted_uses.is_empty() {
        let lines: Vec<&str> = logic_source.lines().collect();
        for &j in &hoisted_uses {
            let src = Some(logic_line_src(j));
            // Only the statement's own first line takes the attribute; a `use foo::{` spanning several lines
            // would otherwise get one per continuation, in the middle of the braced list.
            if lines[j].starts_with("use ") {
                code.push("#[allow(unused_imports)] ", src);
            }
            code.push(lines[j], src);
            code.push("\n", src);
        }
    }
    code.push("\n", None);

    // At file scope, not inside the fn body: a compound component's children name this type from other files,
    // which is the whole reason it exists.
    if let (Some(struct_code), Some((start, _))) = (&context_struct, context_span) {
        for (k, line) in struct_code.lines().enumerate() {
            let src = Some(logic_start0 + (start + k) as u32);
            code.push(line, src);
            code.push("\n", src);
        }
        code.push("\n", None);
    }

    // Emit Props struct at file scope (not inside the fn body) so the type is reachable from the function signature and from other crate files.
    if let Some(struct_code) = &props_struct {
        let struct_start = props_span.map(|(s, _)| s).unwrap_or(0);
        for (k, line) in struct_code.lines().enumerate() {
            // The struct is rebuilt rather than copied — the inline `= default` sugar comes off and the
            // builder's attributes go on — so its lines no longer sit `k` apart from the author's. The
            // rebuild says where each one came from; an attribute it injected came from nowhere, and
            // claiming a line for it would point a diagnostic at whatever the author wrote there.
            let src = match (&props_origins, props_span) {
                (Some(origins), _) => origins
                    .get(k)
                    .copied()
                    .flatten()
                    .map(|line| logic_start0 + line as u32),
                // Synthesized whole: the author wrote no `Props`, so no line of it came from anywhere.
                (None, None) => None,
                (None, Some(_)) => Some(logic_start0 + (struct_start + k) as u32),
            };
            code.push(line, src);
            code.push("\n", src);
        }
        code.push("\n", None);
    }

    if !style_section.is_empty() {
        code.push(&style_section, None);
        if !style_section.ends_with('\n') {
            code.push("\n", None);
        }
        code.push("\n", None);
    }

    code.push("#[allow(dead_code, unused_variables, unused_mut)]\n", None);
    code.push(&signature, None);
    code.push(" {\n", None);
    // One owner per component instance, which is what makes `provide` mean "for my subtree" rather than "for whoever built me": two siblings without one share their parent's scope, so the second is refused as a repeat and reads the first's value. The guard drops at the end of the build, popping the stack and leaving the owner in place for a handler to re-enter.
    code.push("    let __owner = telar::owner_scope();\n", None);
    // use_theme inside the fn so multiple include!-ed files don't conflict at crate scope.
    //
    // `theme` is a handle, not the theme value: a read has to happen inside whatever closure asks for it or
    // it is the theme that was registered when the view was built, forever. So `$theme.primary` is the same
    // `$` that reads a signal, and every rule the markup already has for a read covers a theme read too.
    if uses_theme {
        code.push("    #[allow(unused_imports)] use telar::use_theme;\n", None);
        code.push(
            &format!(
                "    #[allow(unused_variables)] let theme = telar::Theme::<{}>::default();\n",
                input.theme_type.unwrap_or_default()
            ),
            None,
        );
    }
    // The lifted context type, back under the name the author wrote it as. Injected (no `.rsx` line of its
    // own), which is what keeps the body's own lines identical to the source they came from.
    if context_struct.is_some() {
        code.push(
            &format!(
                "    #[allow(unused_imports)] use {} as Context;\n",
                context_type_name(&fn_name)
            ),
            None,
        );
    }

    if !logic.is_empty() {
        // Set by cargo-telar for hot-reload builds (the transpiler runs inside the app's proc macro); keyed signals let the dev host snapshot/restore state across dylib swaps.
        let hot_build = std::env::var("TELAR_HOT_RELOAD_BUILD").is_ok();
        // Argument-context depth carried across lines: a `move` closure sitting at depth 0 starts a statement
        // (a `let clone;` can precede it), but at depth > 0 it is an argument inside an unclosed call/array,
        // where a preceding statement would be invalid Rust — there the clone must wrap the closure instead.
        let mut arg_depth = 0i32;
        for (j, line) in logic.lines().enumerate() {
            let src = Some(logic_line_src(j));
            if hoisted_uses.contains(&j) {
                continue;
            }
            if line.is_empty() {
                code.push("\n", src);
                continue;
            }
            // If this line has a `move` closure that captures a previously declared signal, clone the signal
            // under a mangled name for the closure and rewrite the closure to capture that clone instead — so
            // the original binding stays usable by the view/later logic.
            let mut emitted_line = line.to_string();
            if hot_build
                && let Some(rewritten) =
                    crate::signal_scan::hot_rewrite_signal_decl(&emitted_line, &fn_name)
            {
                emitted_line = rewritten;
            }
            let line_start_depth = arg_depth;
            arg_depth += arg_depth_delta(line);
            if line.contains("move") {
                // `scan_signals` already recorded each signal's declaring line index, so "declared above this line" is a lookup, not a re-parse (and it no longer misses type-annotated `let name: T = signal(...)` bindings).
                let captured: Vec<&str> = signals
                    .iter()
                    .filter(|s| s.line_index < j && contains_ident(line, &s.name))
                    .map(|s| s.name.as_str())
                    .collect();
                if line_start_depth > 0 {
                    // Inside call args: wrap just the closure in a clone block so it stays a valid expression.
                    emitted_line = wrap_closure_clones(&emitted_line, &captured);
                } else {
                    for name in &captured {
                        let mv_name = format!("{name}_rsx_mv");
                        // Injected clone: no `.rsx` counterpart.
                        code.push(&format!("    let {mv_name} = {name}.clone();\n"), None);
                        emitted_line = replace_whole_word(&emitted_line, name, &mv_name);
                    }
                }
            }
            code.push(&format!("    {emitted_line}\n"), src);
        }
        code.push("\n", None);
    }

    // Run the children's recipe now that `[logic]` has built the context to run it in, and hand the result to
    // every `children` placeholder as one `__slots` — so a component with two slots drains one build rather
    // than making one per placeholder. A view that places no children never reaches this and never runs it,
    // which is what makes always taking the recipe cost nothing.
    if has_slot {
        let build = match slot_context_expr(&doc.view.nodes) {
            Some(ctx) => format!("children.build_with({ctx})?"),
            None => "children.build()?".to_string(),
        };
        code.push(&format!("    let mut __slots = {build};\n"), None);
    }

    // The view body carries source markers from generation; resolve them into per-line origins (for diagnostics) plus the byte spans of verbatim expressions. `view_prefix_len` is the body's start offset in the final file, so each span's relative offset rebases onto the generated file.
    let view_prefix_len = code.out.len();
    let resolved = crate::view::resolve_source_map(&view_body);
    for (line, src) in &resolved.lines {
        code.push(line, *src);
        code.push("\n", *src);
    }
    let mut expr_spans: Vec<ExprSpan> = resolved
        .expr_spans
        .iter()
        .map(|&(rel, rsx_start, len)| ExprSpan {
            rsx_start,
            len,
            gen_start: (view_prefix_len + rel) as u32,
        })
        .collect();
    if !code.out.ends_with('\n') {
        code.push("\n", None);
    }
    code.push("}\n", None);

    if !doc.previews.is_empty() {
        // Each preview is its own build fn — so a prop-taking component can be previewed via its markup body — plus a PreviewEntry the bundler collects. The body reuses the view codegen with no signals in scope (a preview has no `[logic]`).
        for (i, preview) in doc.previews.iter().enumerate() {
            let pfn = format!("{fn_name}_preview_{i}");
            let mut pgen =
                ViewGen::with_theme(&doc.style.classes, input.theme_type, input.base_dir);
            let pbody = pgen.generate_root(&preview.body);
            code.push("\n", None);
            code.push("#[allow(dead_code, unused_variables, unused_mut)]\n", None);
            code.push(
                &format!("pub fn {pfn}() -> Result<Box<dyn LayoutItem>, LayoutError> {{\n"),
                None,
            );
            if pgen.uses_theme() {
                code.push("    #[allow(unused_imports)] use telar::use_theme;\n", None);
                code.push(
                    &format!(
                        "    #[allow(unused_variables)] let theme = telar::Theme::<{}>::default();\n",
                        input.theme_type.unwrap_or_default()
                    ),
                    None,
                );
            }
            // `[preview "Name" fixture:path::to::fn]` seeds whatever ambient state this component reads. A path rather than a name declared in `[logic]`, because the logic zone is emitted *inside* the component function while a preview is a sibling function that cannot see into it; the generated module's own `use super::*` resolves a bare name at the crate root. Per-preview only — the process-wide half (theme, locale, config) belongs in the `setup` closure `telar::dev_entry` runs once.
            if let Some(fixture) = preview_fixture(preview) {
                code.push(&format!("    {fixture}();\n"), None);
            }
            let prefix = code.out.len();
            let resolved = crate::view::resolve_source_map(&pbody);
            for (line, src) in &resolved.lines {
                code.push(line, *src);
                code.push("\n", *src);
            }
            for &(rel, rsx_start, len) in &resolved.expr_spans {
                expr_spans.push(ExprSpan {
                    rsx_start,
                    len,
                    gen_start: (prefix + rel) as u32,
                });
            }
            if !code.out.ends_with('\n') {
                code.push("\n", None);
            }
            code.push("}\n", None);
        }

        code.push("\n", None);
        let const_name = preview_entries_const_name(&fn_name);
        code.push(
            &format!("pub const {const_name}: &[::telar::PreviewEntry] = &[\n"),
            None,
        );
        for (i, preview) in doc.previews.iter().enumerate() {
            let pfn = format!("{fn_name}_preview_{i}");
            code.push(
                &format!(
                    "    ::telar::PreviewEntry {{ component_name: \"{fn_name}\", preview_name: \"{}\", build: {pfn}, surface: {} }},\n",
                    preview.name.replace('"', "\\\""),
                    preview_surface(preview)
                ),
                None,
            );
        }
        code.push("];\n", None);
    }

    Ok(TranspiledSource {
        rust_code: code.out,
        preview_names: doc.previews.iter().map(|p| p.name.clone()).collect(),
        source_map: code.map,
        expr_spans,
    })
}

/// Net change in argument-context depth for `line`: `(`/`[` open it, `)`/`]` close it. Block braces `{}` are
/// statement contexts (a `let` is valid inside them), so they don't count. Literals and comments are skipped
/// so brackets inside them don't miscount. Used by the `[logic]` signal-clone pass to tell a statement-start
/// line from a continuation line inside an open call/array.
fn arg_depth_delta(line: &str) -> i32 {
    let bytes = line.as_bytes();
    let mut depth = 0i32;
    let mut i = 0;
    while i < bytes.len() {
        if let Some(end) = literal_or_comment_end(bytes, i) {
            i = end;
            continue;
        }
        match bytes[i] {
            b'(' | b'[' => depth += 1,
            b')' | b']' => depth -= 1,
            _ => {}
        }
        i += 1;
    }
    depth
}

/// Byte index of the first whole-word `move` keyword in `line`, or `None`.
fn find_move_keyword(line: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    let is_word = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let mut from = 0;
    while let Some(rel) = line[from..].find("move") {
        let pos = from + rel;
        let before_ok = pos == 0 || !is_word(bytes[pos - 1]);
        let after_ok = bytes.get(pos + 4).is_none_or(|&b| !is_word(b));
        if before_ok && after_ok {
            return Some(pos);
        }
        from = pos + 4;
    }
    None
}

/// Byte index just past the closure that begins at `start`: scans its body tracking bracket depth and stops
/// at the first depth-0 `,` or the first closing bracket that would pop past the closure's own nesting (i.e.
/// one that closes the *enclosing* call), or end of line. Lets a continuation-line closure argument be
/// wrapped without swallowing the surrounding call's `)`/`,`. Literals and comments are skipped, so a `}` or
/// a `,` written inside one does not end the closure early.
fn closure_end(line: &str, start: usize) -> usize {
    let bytes = line.as_bytes();
    let mut depth = 0i32;
    let mut i = start;
    while i < bytes.len() {
        if let Some(end) = literal_or_comment_end(bytes, i) {
            i = end;
            continue;
        }
        match bytes[i] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' if depth == 0 => break,
            b')' | b']' | b'}' => depth -= 1,
            b',' if depth == 0 => break,
            _ => {}
        }
        i += 1;
    }
    i
}

/// Wraps the `move` closure on `line` in a clone block — `{ let x_rsx_mv = x.clone(); move |..| ..x_rsx_mv.. }`
/// — for every `captured` signal it references, renaming those signals inside the closure. Used when the
/// closure is a call argument (depth > 0), where a preceding `let` statement would be invalid Rust. Returns
/// `line` unchanged when there is no `move` keyword or nothing to capture.
fn wrap_closure_clones(line: &str, captured: &[&str]) -> String {
    if captured.is_empty() {
        return line.to_string();
    }
    let Some(mpos) = find_move_keyword(line) else {
        return line.to_string();
    };
    let end = closure_end(line, mpos);
    let mut inner = line[mpos..end].to_string();
    let mut clones = String::new();
    for name in captured {
        let mv = format!("{name}_rsx_mv");
        inner = replace_whole_word(&inner, name, &mv);
        clones.push_str(&format!("let {mv} = {name}.clone(); "));
    }
    format!("{}{{ {clones}{inner} }}{}", &line[..mpos], &line[end..])
}

/// Whether any node in the view tree is a `children` slot placeholder, so the component function must
/// take a `Slots` argument. Recurses through element children and `if`/`for` branches.
/// The `fixture:` header option of a `[preview]`, if it names one. Quoted or bare, both spellings reach the same
/// path — `fixture:"mock_env"` and `fixture:mock_env` are the same request.
fn preview_fixture(preview: &telar_parser::Preview) -> Option<String> {
    let value = preview
        .options
        .iter()
        .find(|option| option.key == "fixture")?
        .value
        .trim()
        .trim_matches('"');
    (!value.is_empty()).then(|| value.to_string())
}

/// The `surface:WxH` header option of a `[preview]`, as the `Option<PreviewSurface>` its entry carries.
///
/// `[preview "Float" surface:360x240]` renders the component the way the runner mounts a surface — inside a box
/// of that size, under the root that plays the enter transition — instead of as one more widget in the page's
/// column. The bare `animate` flag beside it asks for that transition to run, which is how a preview shows what
/// opening the surface looks like rather than only what it settles to.
fn preview_surface(preview: &telar_parser::Preview) -> String {
    let Some(size) = preview
        .options
        .iter()
        .find(|option| option.key == "surface")
        .map(|option| option.value.trim().trim_matches('"'))
    else {
        return "None".to_string();
    };
    let Some((width, height)) = size
        .split_once(['x', 'X'])
        .and_then(|(w, h)| Some((w.trim().parse::<f32>().ok()?, h.trim().parse::<f32>().ok()?)))
    else {
        // A size that does not parse is a preview the author meant to be a surface, so falling back to a tree
        // would answer a question they did not ask. The generated code names it instead.
        return format!(
            "compile_error!(\"[preview] surface: expects WIDTHxHEIGHT, e.g. surface:360x240 (got {})\")",
            size.replace('"', "'")
        );
    };
    let animate = preview
        .options
        .iter()
        .any(|option| option.key == "animate" && option.value.is_empty());
    format!(
        "Some(::telar::PreviewSurface {{ width: {width:?}, height: {height:?}, animate: {animate} }})"
    )
}

fn view_uses_slot(nodes: &[ViewNode]) -> bool {
    nodes.iter().any(node_uses_slot)
}

fn node_uses_slot(node: &ViewNode) -> bool {
    match node {
        ViewNode::Element(el) => el.tag == "children" || view_uses_slot(&el.children),
        ViewNode::IfBlock(b) => {
            view_uses_slot(&b.then_branch) || b.else_branch.as_deref().is_some_and(view_uses_slot)
        }
        ViewNode::ForBlock(b) => view_uses_slot(&b.body),
        ViewNode::MatchBlock(b) => b.arms.iter().any(|arm| view_uses_slot(&arm.body)),
        ViewNode::LetStmt(_) | ViewNode::Comment(_) => false,
    }
}

/// The `in:` value of a `children` placeholder — the context a compound component builds its children inside.
/// `None` when no placeholder names one, which means the children are built with nothing to read.
///
/// Read off the view rather than off the placeholder that emits it, because the build happens once for the
/// whole component: two `children` placeholders drain one build, exactly as two eager slot placeholders drain
/// one `Slots`.
fn slot_context_expr(nodes: &[ViewNode]) -> Option<String> {
    nodes.iter().find_map(|node| match node {
        ViewNode::Element(el) if el.tag == "children" => el
            .attributes
            .iter()
            .find(|a| a.key == "in")
            .map(|a| a.value.text().trim().to_string())
            .or_else(|| slot_context_expr(&el.children)),
        ViewNode::Element(el) => slot_context_expr(&el.children),
        ViewNode::IfBlock(b) => slot_context_expr(&b.then_branch)
            .or_else(|| b.else_branch.as_deref().and_then(slot_context_expr)),
        ViewNode::ForBlock(b) => slot_context_expr(&b.body),
        ViewNode::MatchBlock(b) => b.arms.iter().find_map(|arm| slot_context_expr(&arm.body)),
        ViewNode::LetStmt(_) | ViewNode::Comment(_) => None,
    })
}

/// Extracts `pub struct Props { … }` (plus any preceding `#[…]` attribute lines) from the logic zone,
/// renames it to `{PascalFnName}Props`, and returns `(struct_code, default_impl, span)`.
/// `default_impl` is `Some` only when the struct uses inline `field: Type = expr` defaults (a synthesized
/// `Default` impl); it is emitted after the struct with no source mapping. `span` is the struct's
/// `[start, end]` (inclusive) line span within `logic`, so the caller can map the struct back to source.
/// The emitted struct, the `.rsx` line each of its lines came from (`None` for a line the transpiler
/// injected), and the span of the declaration lifted out of `[logic]`.
type ExtractedProps = (
    Option<String>,
    Option<Vec<Option<usize>>>,
    Option<(usize, usize)>,
);

/// Whether `line` declares `struct <name>`. The boundary test matters: without it `struct Context` would also
/// claim a `struct ContextMenu`, and `struct Props` a `struct PropsBag`.
fn declares_struct(line: &str, name: &str) -> bool {
    let needle = format!("struct {name}");
    line.find(&needle).is_some_and(|at| {
        !line[at + needle.len()..].starts_with(|c: char| c.is_alphanumeric() || c == '_')
    })
}

/// The inclusive line span of `struct <name> { … }` in `lines`, taking in any `#[…]` attribute and comment
/// lines directly above it: a doc comment left behind would land on whatever statement follows, describing the
/// wrong thing in the one place a reader of the generated crate would look. `None` when the struct is absent
/// or its braces never close.
fn struct_line_span(lines: &[&str], name: &str) -> Option<(usize, usize)> {
    let declared = lines.iter().position(|l| declares_struct(l.trim(), name))?;

    let mut start = declared;
    while start > 0 && {
        let above = lines[start - 1].trim();
        above.starts_with('#') || above.starts_with("//")
    } {
        start -= 1;
    }

    let mut depth = 0i32;
    for (i, line) in lines[declared..].iter().enumerate() {
        for c in line.chars() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some((start, declared + i));
                    }
                }
                _ => {}
            }
        }
    }
    None
}

/// Lifts the `Context` struct of a compound component out of `[logic]`, renamed to `{PascalFnName}Context`.
///
/// Same lifting `Props` gets, and for the same reason plus one: a type declared inside the function body is
/// not nameable from outside it, and a compound component's context exists precisely to be named by the
/// children — which live in other files. The rename is what keeps two compound components in one crate from
/// both re-exporting a type called `Context`.
fn extract_context_struct(logic: &str, fn_name: &str) -> (Option<String>, Option<(usize, usize)>) {
    let lines: Vec<&str> = logic.lines().collect();
    let Some((start, end)) = struct_line_span(&lines, "Context") else {
        return (None, None);
    };
    let renamed = lines[start..=end].join("\n").replace(
        "struct Context",
        &format!("struct {}", context_type_name(fn_name)),
    );
    (Some(renamed), Some((start, end)))
}

fn context_type_name(fn_name: &str) -> String {
    to_pascal_case(fn_name) + "Context"
}

/// Removes the given inclusive line spans from `logic`, keeping every other line untouched.
fn without_line_spans(logic: &str, spans: &[(usize, usize)]) -> String {
    let lines: Vec<&str> = logic.lines().collect();
    lines
        .iter()
        .enumerate()
        .filter(|(i, _)| !spans.iter().any(|&(s, e)| *i >= s && *i <= e))
        .map(|(_, l)| *l)
        .collect::<Vec<_>>()
        .join("\n")
}

fn extract_props_struct(logic: &str, fn_name: &str) -> ExtractedProps {
    let lines: Vec<&str> = logic.lines().collect();

    let Some((start, end)) = struct_line_span(&lines, "Props") else {
        return (None, None, None);
    };

    let struct_code = lines[start..=end].join("\n");
    let props_type = to_pascal_case(fn_name) + "Props";
    // Only rename the struct declaration, not the `derive(Props)` attribute.
    let renamed = struct_code.replace("struct Props", &format!("struct {props_type}"));
    let span = Some((start, end));

    let (open_rel, close_rel) = match (renamed.find('{'), renamed.rfind('}')) {
        (Some(o), Some(c)) if o < c => (o, c),
        _ => return (Some(renamed), None, span),
    };
    let body = &renamed[open_rel + 1..close_rel];
    let parsed: Vec<ParsedField> = split_top_level_commas(body)
        .iter()
        .filter_map(|c| parse_field(c))
        .collect();

    // A struct that derived `Default` meant "every prop may be omitted", which is what `#[props(default)]`
    // says per field. The derive is dropped along with it: nothing constructs these by `Default::default()`
    // any more, and keeping it would demand a `Default` of every field type the builder does not need.
    let header = &renamed[..open_rel];
    let derived_default = header.contains("Default");
    // Per emitted line, the `.rsx` line it came from. An injected attribute has none — claiming one would
    // point a diagnostic at whatever the author happened to write there.
    let mut origins: Vec<Option<usize>> = Vec::new();
    let kept: Vec<(usize, String)> = header
        .lines()
        .enumerate()
        .filter_map(|(k, line)| strip_default_from_derive(line).map(|line| (start + k, line)))
        .collect();
    let mut struct_out = String::new();
    // Everything but the declaration itself, so the injected derive sits against `pub struct` rather than
    // above the doc comment the author wrote for it.
    for (origin, line) in kept.iter().take(kept.len().saturating_sub(1)) {
        struct_out.push_str(line);
        struct_out.push('\n');
        origins.push(Some(*origin));
    }
    struct_out.push_str("#[derive(::telar::Props)]\n");
    origins.push(None);
    if let Some((origin, line)) = kept.last() {
        struct_out.push_str(line.trim_end());
        origins.push(Some(*origin));
    }
    struct_out.push_str(" {\n");
    for f in &parsed {
        // Whatever the author wrote wins: `[logic]` is their Rust, and a `#[props(into)]` they put on a
        // reactive prop is the one thing this cannot work out for them. But `= expr` is theirs too, and it
        // is *merged* rather than dropped — the derive reads one `#[props]` per field, so a prop that had
        // both went out as required and stopped building at every call site that left it off.
        let written = merged_attrs(&f.attrs, f.default.as_deref());
        match (&f.default, derived_default, f.attrs.contains("#[props(")) {
            (_, _, true) => {}
            (Some(expr), _, _) => {
                struct_out.push_str(&format!("    #[props(default = {expr})]\n"));
                origins.push(None);
            }
            (None, true, _) => {
                struct_out.push_str("    #[props(default)]\n");
                origins.push(None);
            }
            (None, false, _) => {}
        }
        for line in written.lines() {
            struct_out.push_str(line);
            struct_out.push('\n');
            origins.push(None);
        }
        struct_out.push_str(&format!("    pub {}: {},\n", f.name, f.ty));
        origins.push(field_line(&lines[start..=end], &f.name).map(|k| start + k));
    }
    struct_out.push('}');
    origins.push(Some(end));

    (Some(struct_out), Some(origins), span)
}

/// Removes `Default` from a `#[derive(...)]` attribute line (returning `None` if the derive becomes
/// empty, so the caller drops the line); any non-derive line passes through unchanged.
fn strip_default_from_derive(line: &str) -> Option<String> {
    let t = line.trim();
    if !t.starts_with("#[derive(") {
        return Some(line.to_string());
    }
    let inner_start = t.find('(')? + 1;
    let inner_end = t.rfind(')')?;
    let items: Vec<&str> = t[inner_start..inner_end]
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && *s != "Default")
        .collect();
    if items.is_empty() {
        return None;
    }
    Some(format!("#[derive({})]", items.join(", ")))
}

/// The line within a struct declaration that declares `name`, so a rebuilt field still points at the one the
/// author wrote rather than at wherever it happened to land in the rebuild.
fn field_line(lines: &[&str], name: &str) -> Option<usize> {
    let wanted = format!("{name}:");
    lines.iter().position(|l| {
        l.trim_start()
            .trim_start_matches("pub ")
            .starts_with(&wanted)
    })
}
