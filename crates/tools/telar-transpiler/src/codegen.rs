//! The codegen engine: turns a parsed [`RsxDocument`] into compilable Rust source, wiring the `[logic]`, `[style]`, `[view]`, and `[preview]` zones together with a per-line source map.

use telar_parser::{RsxDocument, ViewNode};

use crate::error::TranspileError;
use crate::lexer::{contains_ident, literal_or_comment_end};
use crate::signal_scan::{scan_locals, scan_signals};
use crate::source_map::{ExprSpan, ShadowBinding};
use crate::style::generate_style_section;
use crate::tag_errors::glob_import;
use crate::theme_access::ThemeAccess;
use crate::view::ViewGen;
use telar_project::naming::{to_pascal_case, to_snake_case};
use telar_project::{AssetContext, PreludeEntry};

/// A parsed `Props` field: its name, its type, and any inline default expression (the `name: Type = expr` sugar). Whether it is `Option<...>` is no longer anyone's business here — the builder's `some` attribute answers that in the callee's own declaration.
struct ParsedField {
    name: String,
    ty: String,
    default: Option<String>,
    /// Doc lines the author wrote above the field, carried through verbatim so the derive can read them as `#[doc]`.
    docs: String,
    /// Attribute lines the author wrote above the field, carried through verbatim.
    attrs: String,
}

/// Finds the byte index of the top-level `=` that separates a field type from an inline default expression (`name: Type = expr`), or `None`. Skips `=` inside angle brackets and the `==`/`=>`/`<=`/`>=`/`!=` operators, and treats `->` as an arrow (not a generic close) so a type like `Box<dyn Fn() -> T>` keeps correct bracket depth.
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

/// Splits a struct body into field chunks on top-level commas, so a comma inside a default expression (e.g. `= Color::rgba(1.0, 0.0, 0.0, 1.0)`) or a generic (`Vec<A, B>`) does not split a field.
///
/// Comments and string literals are skipped whole. A field's own doc comment is prose, and prose has commas in it — counting those split the field away from its type and dropped it from the struct without a word, which surfaced much later as a missing field at the first call site.
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
/// Column 0 is the test, so a `use` inside a nested `fn` or block stays where its author put it. An unterminated statement is left alone rather than guessed at, so a malformed import fails where it was written.
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

/// The author's `#[props(…)]` with their inline `= expr` folded in, so a prop carrying both keeps one attribute that holds its default.
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

/// Extracts a `Props` field from a `[pub] name: Type[ = default]` chunk, keeping its `///` lines and skipping other comments.
fn parse_field(chunk: &str) -> Option<ParsedField> {
    let docs: String = chunk
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            t.starts_with("///") && !t.starts_with("////")
        })
        .map(|l| format!("    {}\n", l.trim()))
        .collect();
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
        docs,
        attrs,
    })
}

/// Input to a single transpilation: the parsed document plus the desired component function name (typically derived from the source file stem).
pub(crate) struct TranspileInput<'a> {
    pub document: &'a RsxDocument,
    /// The `.rsx` text the document was parsed from, which each preview's source is cut from.
    pub source: &'a str,
    pub component_name: &'a str,
    /// Concrete theme type path (e.g. `SandboxTheme`). When set, the generated view binds `theme` as a `telar::Theme<Type>` handle, which `$theme.field` reads.
    pub theme_type: Option<&'a str>,
    /// The package's baked asset artifact, which static `svg`/`img` paths (`src:"path"`) resolve against. `None` when no package anchors this transpile (e.g. some analyzer/test paths), in which case a static asset yields a `compile_error!`.
    pub assets: Option<&'a AssetContext>,
    /// The package's `[telar] prelude`: each entry is glob-imported between `telar`'s glob and the crate's own, so its items can be named as tags.
    pub prelude: &'a [PreludeEntry],
    /// Whether the package is a `[telar] library`, which cannot name the theme type of the application it is compiled into: `$theme.x` reads the shared `ThemeTokens` as `::telar::use_theme_tokens().x()`, and [`Self::theme_type`] is not consulted.
    pub library: bool,
    /// Emit signal declarations in the keyed form the dev host restores across a dylib swap. An argument rather than an ambient one: this was read from the environment, which made the generated code depend on who ran the process — the editor mirror, the golden snapshots and the build could each produce a different file from the same source and none of them was wrong to.
    pub hot_reload: bool,
    /// Emit a build fn per `[preview]` block, and the entry table naming them.
    ///
    /// Off for anything that ships. A `[preview]` is a thing to look at while writing the component, and it used to reach the binary a user installs: the fns are `pub`, the table is `pub`, and `telar::dev_entry` referenced the table from every `run()` — so the preview bodies were live code, and `TELAR_TEST=1 ./app` ran the preview harness in a shipped application. Not emitting them is the only version of "off" that costs nothing to compile and cannot be reached by an environment variable.
    ///
    /// An argument for the same reason [`Self::hot_reload`] is one: it decides what the generated Rust *is*, so the artifact records which way it was written and a build wanting the other one re-transpiles rather than wiring the wrong shape.
    pub previews: bool,
    /// The `.rsx` file's path relative to its package root, `/`-separated, which each preview entry records as where it is written. `None` when no package anchors this transpile.
    pub rsx_path: Option<&'a str>,
}

/// The generated Rust source for one `.rsx` file.
#[derive(Debug)]
pub struct TranspiledSource {
    pub rust_code: String,
    pub preview_names: Vec<String>,
    /// Per generated line (0-based), the 0-based `.rsx` line it originated from, or `None` for boilerplate and transpiler-injected lines. Lets the analyzer map rust-analyzer's diagnostics on the generated code back onto the `.rsx` source.
    pub source_map: Vec<Option<u32>>,
    /// Byte spans of verbatim `[view]` Rust expressions, mapping a `.rsx` source range to the generated Rust. The half of the map that makes a *column* mean something; persisted into the `.rs.map` beside [`Self::source_map`]. See [`ExprSpan`].
    pub expr_spans: Vec<ExprSpan>,
    /// The bindings the clone pass introduced that stand for source ones. See [`ShadowBinding`].
    pub shadows: Vec<ShadowBinding>,
}

/// Parses `source` and generates Rust for `component_name`, resolving `[style]` colors through `theme_type` when provided so theme switching at runtime takes effect. `assets` is the package's baked artifact, against which static `svg`/`img` `src:"path"` references are resolved.
///
/// Against no `[telar] prelude`: a file that belongs to a package is transpiled through [`crate::transpile_buffer`], which reads everything the package declares.
///
/// **It takes no registry, and that is the point.** A call site used to need the callee's shape — which props it declared, which were optional, which took a closure, whether it accepted children — so every `.rsx` in the workspace had to be scanned before any one of them could be transpiled. Every component takes the same two arguments now, and the props builder answers the rest in the callee's own type, so a file transpiles knowing nothing but itself.
pub fn transpile_source(
    source: &str,
    component_name: &str,
    theme_type: Option<&str>,
    assets: Option<&AssetContext>,
) -> Result<TranspiledSource, TranspileError> {
    let document = telar_parser::parse(source)?;
    transpile(TranspileInput {
        document: &document,
        source,
        component_name,
        theme_type,
        assets,
        prelude: &[],
        library: false,
        hot_reload: false,
        previews: true,
        rsx_path: None,
    })
}

/// Accumulates generated code together with a per-line origin map. Each completed line (terminated by `\n`) records the `.rsx` source line passed when its newline was appended, so callers tag a line by emitting its content and the closing newline with the same `src`.
#[derive(Default)]
pub(crate) struct Code {
    pub(crate) out: String,
    map: Vec<Option<u32>>,
}

impl Code {
    pub(crate) fn push(&mut self, text: &str, src: Option<u32>) {
        for ch in text.chars() {
            self.out.push(ch);
            if ch == '\n' {
                self.map.push(src);
            }
        }
    }
}

/// Binds the `theme` handle an application's `$theme` reads go through, or nothing when there is no theme type, as in a library, whose reads name no binding.
///
/// Inside the fn so multiple `include!`-ed files don't conflict at crate scope. `theme` is a handle, not a value: the read must happen inside the closure that asks, or it freezes at build time.
pub(crate) fn push_theme_binding(code: &mut Code, theme_type: Option<&str>) {
    let Some(theme_type) = theme_type else {
        return;
    };
    code.push("    #[allow(unused_imports)] use telar::use_theme;\n", None);
    code.push(
        &format!(
            "    #[allow(unused_variables)] let theme = telar::Theme::<{theme_type}>::default();\n"
        ),
        None,
    );
}

pub(crate) fn transpile(input: TranspileInput<'_>) -> Result<TranspiledSource, TranspileError> {
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
    // A view declaring no props still gets an empty props type, so every call site can write `XProps::props().build()` unconditionally.
    let props_struct = props_struct.or_else(|| {
        Some(format!(
            "#[derive(::telar::Props)]\npub struct {props_type} {{}}"
        ))
    });
    // Ascending, because the line-number reconstruction below restores them one after another.
    let mut lifted: Vec<(usize, usize)> =
        [props_span, context_span].into_iter().flatten().collect();
    lifted.sort();
    // The body is emitted byte for byte and the lifted type comes back as an alias: renaming it in place would shift every column rustc reports for the line.
    let logic_lifted = without_line_spans(&doc.logic.source, &lifted);
    let logic_source = if lifted.is_empty() {
        &doc.logic.source
    } else {
        &logic_lifted
    };

    let signals = scan_signals(logic_source);

    let theme = ThemeAccess::for_library(input.library);
    let theme_type = match theme {
        ThemeAccess::Tokens => None,
        ThemeAccess::Handle => input.theme_type,
    };
    let style_section = generate_style_section(&doc.style, theme_type, theme);

    let mut locals = scan_locals(logic_source);
    let view_locals_bind_scheme = locals.iter().any(|name| name == "scheme");
    // The recipe is a parameter, not a `[logic]` binding, but a placeholder builds from it wherever it stands, so a closure around one has to clone it in like a local.
    if view_uses_slot(&doc.view.nodes) && !locals.iter().any(|l| l == "children") {
        locals.push("children".to_string());
    }
    let mut view_gen = ViewGen::new(&doc.style.classes, input.assets)
        .with_theme_access(theme)
        .with_locals(locals)
        .with_signals(signals.iter().map(|s| s.name.clone()).collect());
    let view_body = view_gen.generate_root(&doc.view.nodes);
    let scheme_line = scheme_binding(&view_body, view_locals_bind_scheme);

    let logic = logic_source.trim_end().to_string();

    let ret = "Result<Box<dyn LayoutItem>, LayoutError>";
    // One signature for every component removes the need for a registry describing each callee's arity: an unused props builder is empty, and unplaced children never run their recipe.
    let signature = format!("pub fn {fn_name}(props: {props_type}, children: Children) -> {ret}");

    let logic_start0 = doc.logic.start_line.saturating_sub(1) as u32;
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
    // Machine-emitted code nobody can edit, so clippy lints here reach no one who could act on them. Only clippy is suppressed; rustc diagnostics still map back onto the `.rsx`.
    code.push("#![allow(clippy::all)]\n", None);
    // The clone emitter cannot tell `&[T]` from an owned value, and cloning a reference is a no-op rustc warns about.
    code.push("#![allow(noop_method_call)]\n", None);
    code.push(&format!("{}\n", glob_import("telar")), None);
    for entry in input.prelude {
        code.push(&format!("{}\n", glob_import(entry.path())), None);
    }
    // The crate root's own items, so a `[logic]` line can name `core::theme::SandboxTheme`. Deliberately not `use super::*` too, which would make a `.rsx` named after anything in the prelude ambiguous in its siblings.
    code.push(&format!("{}\n", glob_import("crate")), None);

    // `Props` and each `[preview]` are emitted as siblings of the component fn, so a `use` left in the body would be out of scope for exactly the declarations most likely to name an imported type.
    let hoisted_uses = hoisted_use_lines(logic_source);
    if !hoisted_uses.is_empty() {
        let lines: Vec<&str> = logic_source.lines().collect();
        for &j in &hoisted_uses {
            let src = Some(logic_line_src(j));
            // Only the statement's first line takes the attribute; a `use foo::{` spanning lines would otherwise get one per continuation, inside the braced list.
            if lines[j].starts_with("use ") {
                code.push("#[allow(unused_imports)] ", src);
            }
            code.push(lines[j], src);
            code.push("\n", src);
        }
    }
    code.push("\n", None);

    // At file scope, because a compound component's children name this type from other files.
    if let (Some(struct_code), Some((start, _))) = (&context_struct, context_span) {
        for (k, line) in struct_code.lines().enumerate() {
            let src = Some(logic_start0 + (start + k) as u32);
            code.push(line, src);
            code.push("\n", src);
        }
        code.push("\n", None);
    }

    // At file scope, so the type is reachable from the signature and from other crate files.
    if let Some(struct_code) = &props_struct {
        let struct_start = props_span.map(|(s, _)| s).unwrap_or(0);
        for (k, line) in struct_code.lines().enumerate() {
            // The struct is rebuilt rather than copied, so its lines no longer sit `k` apart from the author's. An injected attribute came from nowhere, and claiming a line would point a diagnostic at unrelated source.
            let src = match (&props_origins, props_span) {
                (Some(origins), _) => origins
                    .get(k)
                    .copied()
                    .flatten()
                    .map(|line| logic_start0 + line as u32),
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
    // One owner per instance, so `provide` means "for my subtree": siblings sharing a parent scope would make the second a repeat that reads the first's value.
    code.push("    let __owner = telar::owner_scope();\n", None);
    push_theme_binding(&mut code, theme_type);
    if let Some(binding) = scheme_line {
        code.push(binding, None);
    }
    // Injected, so it has no `.rsx` line of its own and the body's lines stay identical to their source.
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
        let logic_lines: Vec<&str> = logic.lines().collect();
        let mut j = 0;
        while j < logic_lines.len() {
            if logic_lines[j].is_empty() {
                code.push("\n", Some(logic_line_src(j)));
                j += 1;
                continue;
            }
            let end = statement_end(&logic_lines, j);
            let kept: Vec<usize> = (j..end).filter(|k| !hoisted_uses.contains(k)).collect();
            let statement = kept
                .iter()
                .map(|&k| {
                    let line = logic_lines[k];
                    input
                        .hot_reload
                        .then(|| crate::signal_scan::hot_rewrite_signal_decl(line, &fn_name))
                        .flatten()
                        .unwrap_or_else(|| line.to_string())
                })
                .collect::<Vec<_>>()
                .join("\n");
            let mut declared: Vec<&str> = Vec::new();
            for s in signals.iter().filter(|s| s.line_index < j) {
                if !declared.contains(&s.name.as_str()) {
                    declared.push(&s.name);
                }
            }
            let moved = if kept
                .iter()
                .any(|&k| find_move_keyword(logic_lines[k]).is_some())
            {
                crate::rust::moved_reads(&statement, &declared).unwrap_or_default()
            } else {
                Vec::new()
            };
            for name in declared
                .iter()
                .filter(|name| moved.iter().any(|read| read.name == **name))
            {
                code.push(
                    &format!("    let {} = {name}.clone();\n", moved_name(name)),
                    None,
                );
            }
            let statement = rename_moved_reads(&statement, &moved);
            for (&k, line) in kept.iter().zip(statement.split('\n')) {
                if line.is_empty() {
                    code.push("\n", Some(logic_line_src(k)));
                    continue;
                }
                code.push(&format!("    {line}\n"), Some(logic_line_src(k)));
            }
            j = end;
        }
        code.push("\n", None);
    }

    let view_prefix_len = code.out.len();
    let resolved = crate::view::resolve_source_map(&view_body);
    for (line, src) in &resolved.lines {
        code.push(line, *src);
        code.push("\n", *src);
    }
    let mut shadows: Vec<ShadowBinding> = resolved
        .shadows
        .iter()
        .map(|(rel, name)| ShadowBinding {
            gen_decl: (view_prefix_len + rel) as u32,
            name: name.clone(),
        })
        .collect();
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

    if input.previews && (!doc.previews.is_empty() || doc.previews_meta.is_some()) {
        code.push("\n", None);
        code.push(crate::preview::OPEN, None);
        let mapped = crate::preview::emit_variants(
            &mut code,
            &crate::preview::Previews {
                document: doc,
                component: &fn_name,
                props_type: Some(&props_type),
                source: input.source,
                assets: input.assets,
                theme,
                theme_type,
                rsx_path: input.rsx_path,
            },
        );
        expr_spans.extend(mapped.expr_spans);
        shadows.extend(mapped.shadows);
        code.push(crate::preview::CLOSE, None);
    }

    Ok(TranspiledSource {
        rust_code: code.out,
        // What was emitted, not what the document declared: a caller reads this to decide whether to wire a preview table, and a name here with no fn behind it wires one that does not exist.
        preview_names: match input.previews {
            true => doc.previews.iter().map(|p| p.name.clone()).collect(),
            false => Vec::new(),
        },
        source_map: code.map,
        expr_spans,
        shadows,
    })
}

/// Net change in bracket depth for `line` — `(`/`[`/`{` open, `)`/`]`/`}` close — and the last code byte on it, with literals and comments skipped.
fn bracket_delta_and_last_byte(line: &str) -> (i32, Option<u8>) {
    let bytes = line.as_bytes();
    let mut depth = 0i32;
    let mut last = None;
    let mut i = 0;
    while i < bytes.len() {
        if let Some(end) = literal_or_comment_end(bytes, i) {
            if bytes[i] != b'/' {
                last = Some(b'"');
            }
            i = end;
            continue;
        }
        match bytes[i] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            _ => {}
        }
        if !bytes[i].is_ascii_whitespace() {
            last = Some(bytes[i]);
        }
        i += 1;
    }
    (depth, last)
}

/// Index one past the last line of the Rust statement beginning at `lines[start]`. A line comment is a unit of its own, so the clones land below it, next to the statement they serve. A statement ends on a line that leaves every bracket closed and finishes with `;` or `}`, unless the next code line continues it (`.method()`, `?`, `else`). A formatter is free to break `let x =` from its value or a method chain across lines, and the clone `let`s must precede the whole statement, never land inside it.
fn statement_end(lines: &[&str], start: usize) -> usize {
    if lines[start].trim_start().starts_with("//") {
        return start + 1;
    }
    let mut depth = 0i32;
    for (k, line) in lines.iter().enumerate().skip(start) {
        let (delta, last) = bracket_delta_and_last_byte(line);
        depth += delta;
        if depth > 0 || !matches!(last, Some(b';' | b'}')) {
            continue;
        }
        let next = lines[k + 1..]
            .iter()
            .map(|l| l.trim_start())
            .find(|l| !l.is_empty());
        let continues =
            next.is_some_and(|l| l.starts_with('.') || l.starts_with('?') || l.starts_with("else"));
        if !continues {
            return k + 1;
        }
    }
    lines.len()
}

fn moved_name(name: &str) -> String {
    format!("{name}_rsx_mv")
}

/// `statement` with each read a `move` captures pointed at the clone made for it, so the closure takes the clone and the original stays usable after it.
fn rename_moved_reads(statement: &str, reads: &[crate::rust::MovedRead]) -> String {
    let mut renamed = String::with_capacity(statement.len() + reads.len() * 16);
    let mut copied = 0;
    for read in reads {
        renamed.push_str(&statement[copied..read.offset]);
        if read.shorthand {
            renamed.push_str(&read.name);
            renamed.push_str(": ");
        }
        renamed.push_str(&moved_name(&read.name));
        copied = read.offset + read.name.len();
    }
    renamed.push_str(&statement[copied..]);
    renamed
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

/// Whether any node in the view tree is a `children` slot placeholder, so the component function must take a `Slots` argument. Recurses through element children and `if`/`for` branches.
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

/// Extracts `pub struct Props { … }` (plus any preceding `#[…]` attribute lines) from the logic zone, renames it to `{PascalFnName}Props`, and returns `(struct_code, default_impl, span)`. `default_impl` is `Some` only when the struct uses inline `field: Type = expr` defaults (a synthesized `Default` impl); it is emitted after the struct with no source mapping. `span` is the struct's `[start, end]` (inclusive) line span within `logic`, so the caller can map the struct back to source. The emitted struct, the `.rsx` line each of its lines came from (`None` for a line the transpiler injected), and the span of the declaration lifted out of `[logic]`.
type ExtractedProps = (
    Option<String>,
    Option<Vec<Option<usize>>>,
    Option<(usize, usize)>,
);

/// Whether `line` declares `struct <name>`. The boundary test matters: without it `struct Context` would also claim a `struct ContextMenu`, and `struct Props` a `struct PropsBag`.
fn declares_struct(line: &str, name: &str) -> bool {
    let needle = format!("struct {name}");
    line.find(&needle).is_some_and(|at| {
        !line[at + needle.len()..].starts_with(|c: char| c.is_alphanumeric() || c == '_')
    })
}

/// The inclusive line span of `struct <name> { … }` in `lines`, taking in any `#[…]` attribute and comment lines directly above it: a doc comment left behind would land on whatever statement follows, describing the wrong thing in the one place a reader of the generated crate would look. `None` when the struct is absent or its braces never close.
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
/// Same lifting `Props` gets, and for the same reason plus one: a type declared inside the function body is not nameable from outside it, and a compound component's context exists precisely to be named by the children — which live in other files. The rename is what keeps two compound components in one crate from both re-exporting a type called `Context`.
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
    // Only the struct declaration, not the `derive(Props)` attribute.
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

    // A derived `Default` meant "every prop may be omitted", which `#[props(default)]` now says per field. Keeping it would demand a `Default` of every field type the builder does not need.
    let header = &renamed[..open_rel];
    let derived_default = header.contains("Default");
    let mut origins: Vec<Option<usize>> = Vec::new();
    let kept: Vec<(usize, String)> = header
        .lines()
        .enumerate()
        .filter_map(|(k, line)| strip_default_from_derive(line).map(|line| (start + k, line)))
        .collect();
    let mut struct_out = String::new();
    // All but the declaration, so the injected derive sits against `pub struct` rather than above the author's doc comment.
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
        // A `#[props(into)]` the author wrote is the one thing this cannot infer. `= expr` is merged rather than dropped: the derive reads one `#[props]` per field, so a prop with both went out as required.
        let written = merged_attrs(&f.attrs, f.default.as_deref());
        for line in f.docs.lines() {
            struct_out.push_str(line);
            struct_out.push('\n');
            origins.push(None);
        }
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

/// Removes `Default` from a `#[derive(...)]` attribute line (returning `None` if the derive becomes empty, so the caller drops the line); any non-derive line passes through unchanged.
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

/// The line within a struct declaration that declares `name`, so a rebuilt field still points at the one the author wrote rather than at wherever it happened to land in the rebuild.
fn field_line(lines: &[&str], name: &str) -> Option<usize> {
    let wanted = format!("{name}:");
    lines.iter().position(|l| {
        l.trim_start()
            .trim_start_matches("pub ")
            .starts_with(&wanted)
    })
}

/// The module a `*.previews.rsx` compiles to: previews of a component written elsewhere, beside it, under the component's name.
///
/// Everything in it exists for its previews, so its `[logic]` — the `use` lines and the items its previews name, such as a `fixture:` or a `decorator:` — and its `[style]` sit inside the same `::telar::__previews!` as the previews. In a build that emits no previews the module is empty. A `[view]` is refused rather than compiled into a component nothing calls.
pub(crate) fn previews_file(input: TranspileInput<'_>) -> Result<TranspiledSource, TranspileError> {
    let doc = input.document;
    if !doc.view.nodes.is_empty() {
        return Err(TranspileError::Codegen(
            "a `.previews.rsx` holds previews of components written elsewhere, so it has no `[view]`: move the view to a `.rsx` of its own".into(),
        ));
    }
    let mut code = Code::default();
    code.push(
        "// Generated by telar-transpiler — do not edit manually\n",
        None,
    );
    let mut mapped = crate::preview::Mapped::default();
    if input.previews && (!doc.previews.is_empty() || doc.previews_meta.is_some()) {
        let fn_name = to_snake_case(input.component_name);
        let theme = ThemeAccess::for_library(input.library);
        let theme_type = match theme {
            ThemeAccess::Tokens => None,
            ThemeAccess::Handle => input.theme_type,
        };
        code.push("#![allow(clippy::all)]\n", None);
        code.push("#![allow(noop_method_call)]\n", None);
        code.push(&format!("{}\n", glob_import("telar")), None);
        for entry in input.prelude {
            code.push(&format!("{}\n", glob_import(entry.path())), None);
        }
        code.push(&format!("{}\n", glob_import("crate")), None);
        code.push("\n", None);
        code.push(crate::preview::OPEN, None);
        let logic_start0 = doc.logic.start_line.saturating_sub(1) as u32;
        for (j, line) in doc.logic.source.trim_end().lines().enumerate() {
            let src = Some(logic_start0 + j as u32);
            if line.starts_with("use ") {
                code.push("#[allow(unused_imports)] ", src);
            }
            code.push(line, src);
            code.push("\n", src);
        }
        let style_section = generate_style_section(&doc.style, theme_type, theme);
        if !style_section.is_empty() {
            code.push("\n", None);
            code.push(style_section.trim_end(), None);
            code.push("\n", None);
        }
        mapped = crate::preview::emit_variants(
            &mut code,
            &crate::preview::Previews {
                document: doc,
                component: &fn_name,
                props_type: None,
                source: input.source,
                assets: input.assets,
                theme,
                theme_type,
                rsx_path: input.rsx_path,
            },
        );
        code.push(crate::preview::CLOSE, None);
    }
    Ok(TranspiledSource {
        rust_code: code.out,
        preview_names: match input.previews {
            true => doc.previews.iter().map(|p| p.name.clone()).collect(),
            false => Vec::new(),
        },
        source_map: code.map,
        expr_spans: mapped.expr_spans,
        shadows: mapped.shadows,
    })
}

/// The file a directory's `mod.rsx` compiles to: the module itself, not a component in it.
///
/// `[logic]` lands at module level rather than inside a function, which is what gives a `//!` and a `#![…]` somewhere to live — a module is the one place in a `.rsx` where Rust items, not statements, are what belongs. A `[view]`, a `[preview]` or a `[style]` is refused rather than ignored: a module is not callable, so markup here has no caller and silently dropping it would be the surprise.
///
/// The children of the directory are not known here — a file transpiles knowing nothing but itself — so the module ends with an `include!` of the file `cargo telar transpile` writes beside it.
pub fn transpile_module_root(
    source: &str,
    children_file: &str,
) -> Result<TranspiledSource, TranspileError> {
    module_root(&telar_parser::parse(source)?, children_file)
}

/// The same, for a caller that has already parsed.
pub(crate) fn module_root(
    doc: &telar_parser::RsxDocument,
    children_file: &str,
) -> Result<TranspiledSource, TranspileError> {
    if !doc.view.nodes.is_empty() {
        return Err(TranspileError::Codegen(
            "a `mod.rsx` is the directory's module, and a module is not callable: move the `[view]` to a `.rsx` of its own".into(),
        ));
    }
    if !doc.previews.is_empty() || doc.previews_meta.is_some() {
        return Err(TranspileError::Codegen(
            "a `mod.rsx` has no view, so a `[preview]` or `[previews]` here previews nothing: move it beside the component it renders".into(),
        ));
    }
    if !doc.style.classes.is_empty() {
        return Err(TranspileError::Codegen(
            "a `[style]` class is reached by the view that names it, and a `mod.rsx` has none: move it to the component's own `.rsx`".into(),
        ));
    }

    let logic_start0 = doc.logic.start_line.saturating_sub(1) as u32;
    let lines: Vec<&str> = doc.logic.source.trim_end().lines().collect();
    // Everything the author wrote before their first item, which is where rustc requires an inner attribute to be. Emitted with telar's own, above the `use` lines below: a `//!` that landed after one is `E0753`, and it is the reason this mode exists at all.
    let inner_attrs = lines
        .iter()
        .take_while(|line| {
            let code = line.trim_start();
            code.is_empty() || code.starts_with("//!") || code.starts_with("#![")
        })
        .count();

    let mut code = Code::default();
    code.push(
        "// Generated by telar-transpiler — do not edit manually\n",
        None,
    );
    for (j, line) in lines.iter().take(inner_attrs).enumerate() {
        let src = Some(logic_start0 + j as u32);
        code.push(line, src);
        code.push("\n", src);
    }
    code.push("#![allow(clippy::all)]\n", None);
    // No prelude, unlike a component's file: this module hosts Rust the author wrote against their own imports, and a glob here makes a plain `use config::…` ambiguous with whatever the crate root happens to hold (`E0659`). A `mod.rsx` that wants telar's names asks for them.
    for (j, line) in lines.iter().enumerate().skip(inner_attrs) {
        let src = Some(logic_start0 + j as u32);
        code.push(line, src);
        code.push("\n", src);
    }
    code.push(&format!("include!({children_file:?});\n"), None);

    Ok(TranspiledSource {
        rust_code: code.out,
        preview_names: Vec::new(),
        source_map: code.map,
        expr_spans: Vec::new(),
        shadows: Vec::new(),
    })
}

/// The binding behind `$scheme`, for a view that reads it and whose `[logic]` does not bind a `scheme` of its own. A declared `scheme` wins outright, so a component written before the built-in existed keeps meaning what it says.
pub(crate) fn scheme_binding(view_body: &str, shadowed: bool) -> Option<&'static str> {
    (!shadowed && contains_ident(view_body, "scheme"))
        .then_some("    #[allow(unused_variables)] let scheme = telar::ResolvedScheme;\n")
}

#[cfg(test)]
#[path = "codegen_test.rs"]
mod tests;
