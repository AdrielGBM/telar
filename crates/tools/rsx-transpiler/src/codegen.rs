//! The codegen engine: turns a parsed [`RsxDocument`] into compilable Rust source, wiring the `[logic]`, `[style]`, `[view]`, and `[preview]` zones together with a per-line source map.

use std::path::Path;

use rsx_parser::{RsxDocument, ViewNode};

use crate::error::TranspileError;
use crate::naming::{
    contains_ident, preview_entries_const_name, replace_whole_word, to_pascal_case, to_snake_case,
};
use crate::signal_scan::scan_signals;
use crate::style::generate_style_section;
use crate::view::ViewGen;

/// The call-relevant shape of a component: what its function signature and `Props` struct look like,
/// so a *caller* in another `.rsx` can emit the right arguments (optional props, the slot argument)
/// without seeing the callee's source. Collected across the workspace into a [`ComponentRegistry`].
#[derive(Clone, Debug, Default)]
pub struct ComponentSig {
    /// The component declares a `pub struct Props`, so calls must pass a `Props` argument.
    pub has_props: bool,
    /// `Props` derives `Default`, so a call may omit fields (`..Default::default()` fills the rest).
    pub props_default: bool,
    /// The `Props` field names, so a caller knows when it has omitted some (and must default them).
    pub prop_fields: Vec<String>,
    /// The view uses a `children` slot placeholder, so every call must pass a `Slots` argument — even a
    /// childless one (which passes `Slots::new()`).
    pub has_slot: bool,
}

/// Maps a component's callable name (both its path-flattened stem and its bare basename) to its
/// [`ComponentSig`]. Built once per build/analyze pass and threaded into every file's transpile.
pub type ComponentRegistry = std::collections::HashMap<String, ComponentSig>;

/// Scans one `.rsx` source for its [`ComponentSig`] (its `Props` shape and whether it takes a slot).
/// A parse failure yields an empty sig, so a temporarily-broken file never poisons the registry.
pub fn scan_component_sig(source: &str) -> ComponentSig {
    let Ok(doc) = rsx_parser::parse(source) else {
        return ComponentSig::default();
    };
    let (has_props, props_default, prop_fields) = scan_props_struct(&doc.logic.source);
    ComponentSig {
        has_props,
        props_default,
        prop_fields,
        has_slot: view_uses_slot(&doc.view.nodes),
    }
}

/// Scans the logic zone for `struct Props`: returns whether it exists, whether a preceding
/// `#[derive(...)]` lists `Default` (→ optional props), and the field names.
fn scan_props_struct(logic: &str) -> (bool, bool, Vec<String>) {
    let Some(spos) = logic.find("struct Props") else {
        return (false, false, Vec::new());
    };

    // A `#[derive(...Default...)]` on the attribute lines immediately above the struct opts it into defaults.
    let lines: Vec<&str> = logic.lines().collect();
    let sidx = lines
        .iter()
        .position(|l| l.contains("struct Props"))
        .unwrap_or(0);
    let mut derives_default = false;
    let mut i = sidx;
    while i > 0 {
        let t = lines[i - 1].trim();
        if t.is_empty() || t.starts_with("//") {
            i -= 1;
            continue;
        }
        if t.starts_with('#') {
            if t.contains("Default") {
                derives_default = true;
            }
            i -= 1;
            continue;
        }
        break;
    }

    // Field names live between the struct's first `{` and its matching `}`.
    let Some(open_rel) = logic[spos..].find('{') else {
        return (true, derives_default, Vec::new());
    };
    let body_start = spos + open_rel + 1;
    let mut depth = 1i32;
    let mut end = body_start;
    for (i, c) in logic[body_start..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = body_start + i;
                    break;
                }
            }
            _ => {}
        }
    }
    let fields = logic[body_start..end]
        .split(',')
        .filter_map(parse_field_name)
        .collect();
    (true, derives_default, fields)
}

/// Extracts the field name from a `[pub] name: Type` struct-field chunk, skipping comment lines.
fn parse_field_name(chunk: &str) -> Option<String> {
    let cleaned = chunk
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join(" ");
    let t = cleaned.trim();
    let t = t.strip_prefix("pub ").unwrap_or(t).trim_start();
    let colon = t.find(':')?;
    let name = t[..colon].trim();
    (!name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
        .then(|| name.to_string())
}

/// Input to a single transpilation: the parsed document plus the desired component function name (typically derived from the source file stem).
pub(crate) struct TranspileInput<'a> {
    pub document: &'a RsxDocument,
    pub component_name: &'a str,
    /// Concrete theme type path (e.g. `SandboxTheme`). When set, `[style]` color references resolve through `use_theme::<Type>()` instead of `COLOR_*` consts.
    pub theme_type: Option<&'a str>,
    /// Directory of the `.rsx` being transpiled, used to resolve static `svg`/`img` asset paths (`src:"path"`) for build-time baking. `None` when no filesystem anchor is available (e.g. some analyzer/test paths), in which case a static asset yields a `compile_error!`.
    pub base_dir: Option<&'a Path>,
    /// Signatures of every component in the workspace, so a call site emits optional props and the slot argument correctly. `None` (tests, isolated transpiles) falls back to the per-file heuristic: pass a slot arg only when markup children are present, and require every prop field.
    pub registry: Option<&'a ComponentRegistry>,
}

/// The generated Rust source for one `.rsx` file.
pub struct TranspiledSource {
    pub rust_code: String,
    pub preview_names: Vec<String>,
    /// Per generated line (0-based), the 0-based `.rsx` line it originated from, or `None` for boilerplate and transpiler-injected lines. Lets the analyzer map rust-analyzer's diagnostics on the generated code back onto the `.rsx` source.
    pub source_map: Vec<Option<u32>>,
    /// Byte spans of verbatim `[view]` Rust expressions, mapping a `.rsx` source range to the generated Rust. In-memory only (not serialized, not part of the `.rs.map`): the analyzer uses them to offer Rust completion inside `[view]` expressions. See [`ExprSpan`].
    pub expr_spans: Vec<ExprSpan>,
    /// Whether the component takes a `Props` argument, so callers can alias its `Props` type by base name.
    pub has_props: bool,
}

/// A `[view]` Rust expression that is copied byte-for-byte from the `.rsx` source into the generated Rust, so `gen_start + (cursor_byte - rsx_start)` maps a `.rsx` cursor onto the generated file on a UTF-8 char boundary. Only emitted for verbatim fragments (interpolation `{expr}`, `if`/`let` expressions, verbatim closure / pass-through attr values); non-verbatim ones (`for` re-tokenized patterns, transformed numeric/color attrs) produce no span.
pub struct ExprSpan {
    /// Byte offset of the fragment's start in the `.rsx` source.
    pub rsx_start: u32,
    /// Byte length of the fragment (identical in source and generated).
    pub len: u32,
    /// Byte offset of the fragment's start in the generated Rust.
    pub gen_start: u32,
}

/// Serializes a [`TranspiledSource::source_map`] as a JSON array (`[null,3,3,...]`), the format the editor extension reads to map rust-analyzer's diagnostics on the generated Rust back to `.rsx`.
pub fn source_map_to_json(map: &[Option<u32>]) -> String {
    let mut out = String::with_capacity(map.len() * 3 + 2);
    out.push('[');
    for (i, entry) in map.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        match entry {
            Some(line) => out.push_str(&line.to_string()),
            None => out.push_str("null"),
        }
    }
    out.push(']');
    out
}

/// Parses `source` and generates Rust for `component_name`, resolving `[style]` colors through `theme_type` when provided so theme switching at runtime takes effect. `base_dir` is the directory of the `.rsx` (its parent), against which static `svg`/`img` asset paths are resolved and baked at build time.
pub fn transpile_source_with_theme(
    source: &str,
    component_name: &str,
    theme_type: Option<&str>,
    base_dir: Option<&Path>,
) -> Result<TranspiledSource, TranspileError> {
    transpile_source_full(source, component_name, theme_type, base_dir, None)
}

/// Like [`transpile_source_with_theme`], but also given the workspace [`ComponentRegistry`] so calls to
/// other components emit optional props and the slot argument correctly (a childless call to a slotted
/// component still passes `Slots::new()`; a call that omits defaultable props adds `..Default::default()`).
pub fn transpile_source_full(
    source: &str,
    component_name: &str,
    theme_type: Option<&str>,
    base_dir: Option<&Path>,
    registry: Option<&ComponentRegistry>,
) -> Result<TranspiledSource, TranspileError> {
    let document = rsx_parser::parse(source)?;
    transpile(TranspileInput {
        document: &document,
        component_name,
        theme_type,
        base_dir,
        registry,
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

    let (props_struct, logic_no_props, struct_span) =
        extract_props_struct(&doc.logic.source, &fn_name);
    let has_props = props_struct.is_some();
    let props_type = if has_props {
        to_pascal_case(&fn_name) + "Props"
    } else {
        String::new()
    };
    let logic_source = if has_props {
        &logic_no_props
    } else {
        &doc.logic.source
    };

    let signals = scan_signals(logic_source);

    let style_section = generate_style_section(&doc.style, input.theme_type.is_some());

    let mut view_gen = ViewGen::with_theme(
        &doc.style.classes,
        &doc.style.constants,
        input.theme_type,
        input.base_dir,
    )
    .with_registry(input.registry);
    let view_body = view_gen.generate_root(&doc.view.nodes);
    let uses_theme = view_gen.uses_theme();

    let logic = logic_source.trim_end();

    // A `children` placeholder anywhere in the view makes the component take a `Slots` argument.
    let has_slot = view_uses_slot(&doc.view.nodes);
    let ret = "Result<Box<dyn LayoutItem>, LayoutError>";
    let signature = match (has_props, has_slot) {
        (true, true) => format!(
            "pub fn {fn_name}(ctx: &mut WidgetCtx, props: {props_type}, mut __slots: Slots) -> {ret}"
        ),
        (true, false) => {
            format!("pub fn {fn_name}(ctx: &mut WidgetCtx, props: {props_type}) -> {ret}")
        }
        (false, true) => {
            format!("pub fn {fn_name}(ctx: &mut WidgetCtx, mut __slots: Slots) -> {ret}")
        }
        (false, false) => format!("pub fn {fn_name}(ctx: &mut WidgetCtx) -> {ret}"),
    };

    // 0-based `.rsx` line of `logic_source` line 0, used to map generated lines back to the source.
    let logic_start0 = doc.logic.start_line.saturating_sub(1) as u32;
    let struct_len = struct_span.map(|(s, e)| e - s + 1).unwrap_or(0);
    let struct_start = struct_span.map(|(s, _)| s).unwrap_or(0);
    // `logic_source` (props-struct removed when present) line index -> its 0-based `.rsx` line.
    let logic_line_src = |j: usize| -> u32 {
        let orig = if struct_span.is_some() && j >= struct_start {
            j + struct_len
        } else {
            j
        };
        logic_start0 + orig as u32
    };

    let mut code = Code::default();
    code.push(
        "// Generated by rsx-transpiler — do not edit manually\n",
        None,
    );
    code.push("#[allow(unused_imports)] use rsx::*;\n", None);
    // Each `.rsx` is wired as its own `mod` (so rust-analyzer treats it as a real module and offers completion); `use super::*` re-imports the sibling components the host re-exports, so cross-component calls like `feature_card(ctx)` resolve by bare name just as they did under the old `include!`.
    code.push("#[allow(unused_imports)] use super::*;\n", None);
    code.push("\n", None);

    // Emit Props struct at file scope (not inside the fn body) so the type is reachable from the function signature and from other crate files.
    if let Some(struct_code) = &props_struct {
        for (k, line) in struct_code.lines().enumerate() {
            let src = Some(logic_start0 + (struct_start + k) as u32);
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
    // use_theme inside the fn so multiple include!-ed files don't conflict at crate scope.
    if uses_theme {
        code.push("    #[allow(unused_imports)] use rsx::use_theme;\n", None);
    }

    if !logic.is_empty() {
        // Set by cargo-rsx for hot-reload builds (the transpiler runs inside the app's proc macro); keyed signals let the dev host snapshot/restore state across dylib swaps.
        let hot_build = std::env::var("RSX_HOT_RELOAD_BUILD").is_ok();
        let mut declared: Vec<&str> = Vec::new();
        for (j, line) in logic.lines().enumerate() {
            let src = Some(logic_line_src(j));
            if line.is_empty() {
                code.push("\n", src);
                continue;
            }
            // If this line has a `move` closure that captures a previously declared signal, emit a dedicated clone with a mangled name for the closure, then rewrite the line so the closure captures that clone instead of the original. This leaves the original binding intact for the view code.
            let mut emitted_line = line.to_string();
            if hot_build
                && let Some(rewritten) =
                    crate::signal_scan::hot_rewrite_signal_decl(&emitted_line, &fn_name)
            {
                emitted_line = rewritten;
            }
            if line.contains("move") {
                for sig_name in &declared {
                    if contains_ident(line, sig_name) {
                        let mv_name = format!("{sig_name}_rsx_mv");
                        // Injected clone: no `.rsx` counterpart.
                        code.push(&format!("    let {mv_name} = {sig_name}.clone();\n"), None);
                        emitted_line = replace_whole_word(&emitted_line, sig_name, &mv_name);
                    }
                }
            }
            code.push(&format!("    {emitted_line}\n"), src);
            for sig in &signals {
                let decl_prefix = format!("let {} =", sig.name);
                let decl_prefix_mut = format!("let mut {} =", sig.name);
                if line.trim_start().starts_with(&decl_prefix)
                    || line.trim_start().starts_with(&decl_prefix_mut)
                {
                    declared.push(&sig.name);
                }
            }
        }
        code.push("\n", None);
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
            let mut pgen = ViewGen::with_theme(
                &doc.style.classes,
                &doc.style.constants,
                input.theme_type,
                input.base_dir,
            )
            .with_registry(input.registry);
            let pbody = pgen.generate_root(&preview.body);
            code.push("\n", None);
            code.push("#[allow(dead_code, unused_variables, unused_mut)]\n", None);
            code.push(
                &format!(
                    "pub fn {pfn}(ctx: &mut WidgetCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {{\n"
                ),
                None,
            );
            if pgen.uses_theme() {
                code.push("    #[allow(unused_imports)] use rsx::use_theme;\n", None);
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
            &format!("pub const {const_name}: &[::rsx::PreviewEntry] = &[\n"),
            None,
        );
        for (i, preview) in doc.previews.iter().enumerate() {
            let pfn = format!("{fn_name}_preview_{i}");
            code.push(
                &format!(
                    "    ::rsx::PreviewEntry {{ component_name: \"{fn_name}\", preview_name: \"{}\", build: {pfn} }},\n",
                    preview.name.replace('"', "\\\"")
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
        has_props,
    })
}

/// Whether any node in the view tree is a `children` slot placeholder, so the component function must
/// take a `Slots` argument. Recurses through element children and `if`/`for` branches.
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
        ViewNode::LetStmt { .. } => false,
    }
}

/// Extracts `pub struct Props { … }` (plus any preceding `#[…]` attribute lines) from the logic zone, renames it to `{PascalFnName}Props`, and returns the renamed struct code together with the logic zone with the struct removed.
///
/// Returns `(None, original_logic, None)` when no `struct Props` is found; otherwise the third element is the `[start, end]` (inclusive) line span of the struct within `logic`'s lines, so the caller can map the emitted struct back to the original source.
fn extract_props_struct(
    logic: &str,
    fn_name: &str,
) -> (Option<String>, String, Option<(usize, usize)>) {
    let lines: Vec<&str> = logic.lines().collect();

    let Some(struct_line) = lines.iter().position(|l| l.trim().contains("struct Props")) else {
        return (None, logic.to_string(), None);
    };

    // Include any preceding `#[…]` attribute lines (e.g. `#[derive(Props)]`).
    let mut start = struct_line;
    while start > 0 && lines[start - 1].trim().starts_with('#') {
        start -= 1;
    }

    // Scan from the `struct Props` line, counting braces to find the closing `}`.
    let mut depth = 0i32;
    let mut end = struct_line;
    let mut found_close = false;
    for (i, line) in lines[struct_line..].iter().enumerate() {
        for c in line.chars() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        found_close = true;
                    }
                }
                _ => {}
            }
        }
        if found_close {
            end = struct_line + i;
            break;
        }
    }

    if !found_close {
        return (None, logic.to_string(), None);
    }

    let struct_code = lines[start..=end].join("\n");
    let props_type = to_pascal_case(fn_name) + "Props";
    // Only rename the struct declaration, not the `derive(Props)` attribute.
    let renamed = struct_code.replace("struct Props", &format!("struct {props_type}"));

    let mut remaining_lines = lines[..start].to_vec();
    remaining_lines.extend_from_slice(&lines[end + 1..]);
    (
        Some(renamed),
        remaining_lines.join("\n"),
        Some((start, end)),
    )
}
