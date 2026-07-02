//! RSX transpiler: converts a parsed [`RsxDocument`] AST into compilable Rust source code that depends on `rsx::*`.

mod error;
pub mod naming;
mod registry;
mod signal_scan;
mod style;
mod transition;
mod view;

pub use error::TranspileError;
pub use registry::{
    TAG_REFERENCES_VARIABLE, builtin_tags, color_attr_keys, is_builtin_tag,
    is_control_flow_keyword, layout_attr_keys, tag_attr_keys,
};
pub use signal_scan::{SignalInfo, scan_signals};

use std::path::{Path, PathBuf};

use rsx_parser::RsxDocument;

use crate::naming::{
    contains_ident, preview_entries_const_name, replace_whole_word, to_pascal_case, to_snake_case,
};
use crate::style::generate_style_section;
use crate::view::ViewGen;

/// Input to a single transpilation: the parsed document plus the desired component function name (typically derived from the source file stem).
pub(crate) struct TranspileInput<'a> {
    pub document: &'a RsxDocument,
    pub component_name: &'a str,
    /// Concrete theme type path (e.g. `SandboxTheme`). When set, `[style]` color references resolve through `use_theme::<Type>()` instead of `COLOR_*` consts.
    pub theme_type: Option<&'a str>,
}

/// The generated Rust source for one `.rsx` file.
pub struct TranspiledSource {
    pub rust_code: String,
    pub preview_names: Vec<String>,
    /// Per generated line (0-based), the 0-based `.rsx` line it originated from, or `None` for boilerplate and transpiler-injected lines. Lets the analyzer map rust-analyzer's diagnostics on the generated code back onto the `.rsx` source.
    pub source_map: Vec<Option<u32>>,
    /// Byte spans of verbatim `[view]` Rust expressions, mapping a `.rsx` source range to the generated Rust. In-memory only (not serialized, not part of the `.rs.map`): the analyzer uses them to offer Rust completion inside `[view]` expressions. See [`ExprSpan`].
    pub expr_spans: Vec<ExprSpan>,
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

/// Parses `source` and generates Rust for `component_name`, resolving `[style]` colors through `theme_type` when provided so theme switching at runtime takes effect.
pub fn transpile_source_with_theme(
    source: &str,
    component_name: &str,
    theme_type: Option<&str>,
) -> Result<TranspiledSource, TranspileError> {
    let document = rsx_parser::parse(source)?;
    transpile(TranspileInput {
        document: &document,
        component_name,
        theme_type,
    })
}

/// Recursively collects files with `extension` under `dir`, descending into a subdirectory only when `keep_dir` returns true for its name. The result is sorted.
pub fn collect_files_by_ext(
    dir: &Path,
    extension: &str,
    keep_dir: &dyn Fn(&str) -> bool,
) -> Vec<PathBuf> {
    fn walk(
        dir: &Path,
        extension: &str,
        keep_dir: &dyn Fn(&str) -> bool,
        result: &mut Vec<PathBuf>,
    ) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let skip = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|name| !keep_dir(name));
                if !skip {
                    walk(&path, extension, keep_dir, result);
                }
            } else if path.extension().and_then(|e| e.to_str()) == Some(extension) {
                result.push(path);
            }
        }
    }

    let mut result = Vec::new();
    walk(dir, extension, keep_dir, &mut result);
    result.sort();
    result
}

pub fn find_rsx_files(dir: &Path) -> Vec<PathBuf> {
    collect_files_by_ext(dir, "rsx", &|_| true)
}

/// Derives a unique stem for a `.rsx` file from its path relative to `src_dir`, flattening subdirectories with `_` so files in different directories don't collide (e.g. `src/components/button.rsx` -> `components_button`).
pub fn relative_stem(path: &Path, src_dir: &Path) -> String {
    let rel = path.strip_prefix(src_dir).unwrap_or(path);
    let without_ext = rel.with_extension("");
    without_ext
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("_")
}

/// Derives the output `.rs` path (relative to the output root) for a `.rsx` file by mirroring its location under `src_dir`, so files in different directories never collide (e.g. `src/components/button.rsx` -> `components/button.rs`). Used for the transpiler's `.rsx/build/` output. Returns `None` for files outside `src_dir`: those are never transpiled, so they have no place in the build tree — and flattening their absolute path would escape the output root entirely.
pub fn relative_output_path(path: &Path, src_dir: &Path) -> Option<PathBuf> {
    let rel = path.strip_prefix(src_dir).ok()?;
    if rel.as_os_str().is_empty() {
        return None;
    }
    Some(rel.with_extension("rs"))
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

    let mut view_gen =
        ViewGen::with_theme(&doc.style.classes, &doc.style.constants, input.theme_type);
    let view_body = view_gen.generate_root(&doc.view.nodes);
    let uses_theme = view_gen.uses_theme();

    let logic = logic_source.trim_end();

    let signature = if has_props {
        format!(
            "pub fn {fn_name}(ctx: &mut WidgetCtx, props: {props_type}) -> Result<Box<dyn LayoutItem>, LayoutError>"
        )
    } else {
        format!("pub fn {fn_name}(ctx: &mut WidgetCtx) -> Result<Box<dyn LayoutItem>, LayoutError>")
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
            let mut pgen =
                ViewGen::with_theme(&doc.style.classes, &doc.style.constants, input.theme_type);
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
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_map_points_generated_logic_back_to_rsx() {
        // rsx lines (1-based): 1 [logic], 2 derive, 3 struct, 4 body field, 5 close, 6 let, 8 [view], 9 col.
        let src = "[logic]\n#[derive(Props)]\npub struct Props {\n    pub body: &'static st,\n}\nlet count = signal(0i32);\n\n[view]\ncol\n";
        let result = transpile_source_with_theme(src, "demo", None).unwrap();
        let lines: Vec<&str> = result.rust_code.lines().collect();
        assert_eq!(lines.len(), result.source_map.len());

        // The field with the `st` typo maps back to its `.rsx` line (4 -> 0-based 3).
        let body_idx = lines
            .iter()
            .position(|l| l.contains("&'static st"))
            .expect("generated struct field");
        assert_eq!(result.source_map[body_idx], Some(3));

        // The logic binding maps back to its `.rsx` line (6 -> 0-based 5).
        let let_idx = lines
            .iter()
            .position(|l| l.contains("signal"))
            .expect("generated logic line");
        assert_eq!(result.source_map[let_idx], Some(5));

        // Boilerplate (the prelude `use`) has no source line.
        let use_idx = lines.iter().position(|l| l.contains("use rsx::*")).unwrap();
        assert_eq!(result.source_map[use_idx], None);
    }

    #[test]
    fn source_map_points_generated_view_back_to_rsx() {
        // rsx lines (1-based): 1 [view], 2 col, 3 text, 4 row, 5 btn (with closure).
        let src =
            "[view]\ncol\n    text \"hi\"\n    row\n        btn \"+\" on_press:|| missing.set(1)\n";
        let result = transpile_source_with_theme(src, "demo", None).unwrap();
        let lines: Vec<&str> = result.rust_code.lines().collect();
        assert_eq!(lines.len(), result.source_map.len());

        // No source marker leaks into the generated output.
        assert!(!result.rust_code.contains("@RSX@"));

        // The button's closure line (where `missing` would error) maps to the `btn` line (5 -> 4).
        let btn_idx = lines
            .iter()
            .position(|l| l.contains("missing.set(1)"))
            .expect("generated button closure");
        assert_eq!(result.source_map[btn_idx], Some(4));

        // The text leaf maps to its own line (3 -> 2).
        let text_idx = lines
            .iter()
            .position(|l| l.contains("\"hi\""))
            .expect("generated text leaf");
        assert_eq!(result.source_map[text_idx], Some(2));

        // A container's own closing constructor maps back to the container, not its last child: the row's `Container::new` resolves to the `row` line (4 -> 3) even though the btn nested inside.
        let row_ctor = lines
            .iter()
            .position(|l| l.contains("flex_row()"))
            .expect("generated row container");
        assert_eq!(result.source_map[row_ctor], Some(3));
    }

    // COUNTER declares [style] colors: with no theme they become local COLOR_* consts; with a theme_type they resolve through use_theme instead (see the theme tests below).
    const COUNTER: &str = r#"[logic]
let count = signal(0i32);

[style]
primary: #3d78fa
dark: #141424

@card
    width: 240
    padding: 20
    gap: 12
    direction: col
    align: center

[view]
col @card
    text "Count: {$count}" size:14 color:dark
    btn "Increment" fill:primary on_press:|| $count.update(|n| *n += 1)
"#;

    // COUNTER_THEMED has no [style] color declarations — colors flow through the live theme so they react to `set_theme_with_widgets(...)` calls at runtime.
    const COUNTER_THEMED: &str = r#"[logic]
let count = signal(0i32);

[style]
@card
    width: 240
    padding: 20
    gap: 12
    direction: col
    align: center

[view]
col @card
    text "Count: {$count}" size:14 color:dark
    btn "Increment" fill:primary on_press:|| $count.update(|n| *n += 1)
"#;

    #[test]
    fn relative_output_path_mirrors_tree_and_rejects_out_of_src() {
        let src = Path::new("/proj/src");
        // Nested file mirrors its location relative to src/.
        assert_eq!(
            relative_output_path(Path::new("/proj/src/sections/cards.rsx"), src),
            Some(PathBuf::from("sections/cards.rs"))
        );
        // Root-level file stays at the output root.
        assert_eq!(
            relative_output_path(Path::new("/proj/src/counter.rsx"), src),
            Some(PathBuf::from("counter.rs"))
        );
        // Out-of-src files have no mirror — flattening their absolute path would escape the output root.
        assert_eq!(
            relative_output_path(Path::new("/proj/examples/foo.rsx"), src),
            None
        );
        // src/ itself is not a file.
        assert_eq!(relative_output_path(src, src), None);
    }

    #[test]
    fn generates_counter() {
        let out = transpile_source_with_theme(COUNTER, "counter", None).unwrap();
        let code = out.rust_code;
        assert!(code.contains("pub fn counter(ctx: &mut WidgetCtx)"));
        // [style]-declared colors become local constants.
        assert!(code.contains("const COLOR_PRIMARY: Color = Color::rgba(61.0 / 255.0"));
        assert!(code.contains("fn style_card() -> LayoutStyle"));
        assert!(code.contains("move || format!(\"Count: {}\", { count.get() })"));
        assert!(code.contains("Button::new(ctx, \"Increment\")?"));
        assert!(code.contains(".on_click(move || count.update(|n| *n += 1))"));
        assert!(code.contains("Container::new(ctx, style_card(), children!["));
        assert!(code.contains("Ok(Box::new(__col_0))"));
    }

    #[test]
    fn section_and_heading_expand_to_primitives() {
        let src = "[view]\nsection \"Cards\"\n    heading \"Subtitle\"\n    text \"Body\" size:14 color:dark\n";
        let code = transpile_source_with_theme(src, "cards", None)
            .unwrap()
            .rust_code;
        // `section`/`heading` no longer reference removed library components.
        assert!(
            !code.contains("Section::new") && !code.contains("Heading::new"),
            "section/heading must not reference removed components in:\n{code}"
        );
        // `section` becomes a muted-heading Text inside a flex-column Container.
        assert!(
            code.contains("Container::new(ctx, LayoutStyle::new().flex_column().gap(8.0)"),
            "expected section's flex-column Container in:\n{code}"
        );
        // `heading` becomes a Text colored from the theme's widget_muted token.
        assert!(
            code.contains("use_widget_theme().map(|t| t.widget_muted())"),
            "expected heading's muted style in:\n{code}"
        );
        assert!(
            code.contains("TextStyle::new(12.0, color)"),
            "expected heading's 12px caption in:\n{code}"
        );
    }

    #[test]
    fn theme_type_resolves_colors_via_use_theme() {
        // Colors not declared in [style] resolve reactively through the theme.
        let out =
            transpile_source_with_theme(COUNTER_THEMED, "counter", Some("SandboxTheme")).unwrap();
        let code = out.rust_code;
        assert!(code.contains("use rsx::use_theme;"));
        assert!(code.contains("use_theme::<SandboxTheme>().dark"));
        assert!(code.contains("use_theme::<SandboxTheme>().primary"));
        // No COLOR_* consts should be referenced inside the function body.
        let fn_start = code.find("pub fn counter").unwrap();
        assert!(!code[fn_start..].contains("COLOR_DARK"));
        assert!(!code[fn_start..].contains("COLOR_PRIMARY"));
    }

    #[test]
    fn style_declared_colors_resolve_via_theme_when_active() {
        // With a theme configured, [style]-declared colors resolve reactively through use_theme like undeclared ones, and their now-dead COLOR_* consts are omitted.
        let out = transpile_source_with_theme(COUNTER, "counter", Some("SandboxTheme")).unwrap();
        let code = out.rust_code;
        assert!(code.contains("use_theme::<SandboxTheme>().primary"));
        assert!(code.contains("use_theme::<SandboxTheme>().dark"));
        // The now-unused color consts are not emitted.
        assert!(!code.contains("const COLOR_PRIMARY"));
        assert!(!code.contains("const COLOR_DARK"));
        let fn_start = code.find("pub fn counter").unwrap();
        assert!(!code[fn_start..].contains("COLOR_PRIMARY"));
    }

    #[test]
    fn extracts_props_struct_to_file_scope() {
        let src = "[logic]\n#[derive(Props)]\npub struct Props { pub title: &'static str }\n[view]\ntext \"hi\"\n";
        let out = transpile_source_with_theme(src, "card", None).unwrap();
        let code = &out.rust_code;
        // Props struct is renamed and lifted before the fn declaration.
        assert!(
            code.contains("pub struct CardProps"),
            "struct should be renamed CardProps"
        );
        assert!(
            code.contains("pub fn card(ctx: &mut WidgetCtx, props: CardProps)"),
            "fn signature must use CardProps"
        );
        // The struct must appear before the fn, not inside it.
        let struct_pos = code.find("pub struct CardProps").unwrap();
        let fn_pos = code.find("pub fn card").unwrap();
        assert!(
            struct_pos < fn_pos,
            "Props struct must precede the function"
        );
        // The derive attribute is preserved as-is (it references the macro, not the struct name).
        assert!(
            code.contains("#[derive(Props)]"),
            "derive attribute must be preserved"
        );
    }

    #[test]
    fn expr_spans_map_interpolation_verbatim() {
        // `[logic]` line 2 declares `count`; `[view]` line 4 interpolates it.
        let src = "[logic]\nlet count = signal(0i32);\n[view]\ntext \"Count: {count}\"\n";
        let out = transpile_source_with_theme(src, "demo", None).unwrap();

        // No marker leaks into the generated Rust.
        assert!(!out.rust_code.contains("@RSX@"), "markers must be stripped");

        // One span, for the `{count}` interpolation.
        assert_eq!(out.expr_spans.len(), 1, "expected one expression span");
        let span = &out.expr_spans[0];

        // The span's source range is exactly `count` in the original `.rsx`.
        let rsx_frag = &src[span.rsx_start as usize..(span.rsx_start + span.len) as usize];
        assert_eq!(rsx_frag, "count");

        // It maps byte-for-byte onto `count` in the generated Rust (char-boundary safe).
        let gen_frag =
            &out.rust_code[span.gen_start as usize..(span.gen_start + span.len) as usize];
        assert_eq!(gen_frag, "count");
        assert!(out.rust_code.is_char_boundary(span.gen_start as usize));
    }

    #[test]
    fn expr_spans_are_char_boundary_safe_with_multibyte() {
        // A multi-byte literal precedes the interpolation; the span must still land on char boundaries in both source and generated (the byte-wise affine map would otherwise mis-slice / panic).
        let src = "[logic]\nlet count = signal(0i32);\n[view]\ntext \"caf\u{e9} {count}\"\n";
        let out = transpile_source_with_theme(src, "demo", None).unwrap();
        assert_eq!(out.expr_spans.len(), 1);
        let span = &out.expr_spans[0];
        let (rs, re) = (
            span.rsx_start as usize,
            (span.rsx_start + span.len) as usize,
        );
        let (gs, ge) = (
            span.gen_start as usize,
            (span.gen_start + span.len) as usize,
        );
        assert!(src.is_char_boundary(rs) && src.is_char_boundary(re));
        assert!(out.rust_code.is_char_boundary(gs) && out.rust_code.is_char_boundary(ge));
        assert_eq!(&src[rs..re], "count");
        assert_eq!(&out.rust_code[gs..ge], "count");
    }

    #[test]
    fn expr_spans_cover_if_and_let() {
        let src = "[logic]\n[view]\ncol\n    let n = 1\n    if n > 0\n        text \"hi\"\n";
        let out = transpile_source_with_theme(src, "demo", None).unwrap();
        assert!(!out.rust_code.contains("@RSX@"));

        // Each span must point at the identical fragment in both source and generated output.
        for span in &out.expr_spans {
            let rsx_frag = &src[span.rsx_start as usize..(span.rsx_start + span.len) as usize];
            let gen_frag =
                &out.rust_code[span.gen_start as usize..(span.gen_start + span.len) as usize];
            assert_eq!(rsx_frag, gen_frag, "span fragment must be verbatim");
        }
        let frags: Vec<&str> = out
            .expr_spans
            .iter()
            .map(|s| &src[s.rsx_start as usize..(s.rsx_start + s.len) as usize])
            .collect();
        assert!(frags.contains(&"let n = 1"), "let span missing: {frags:?}");
        assert!(
            frags.contains(&"n > 0"),
            "if-condition span missing: {frags:?}"
        );
    }

    #[test]
    fn component_with_quoted_string_attr() {
        let src = "[logic]\n[view]\nmy_widget label:\"hello\" size:16\n";
        let out = transpile_source_with_theme(src, "demo", None).unwrap();
        let code = &out.rust_code;
        assert!(
            code.contains("my_widget(ctx, crate::MyWidgetProps {"),
            "should call component fn with Props"
        );
        assert!(
            code.contains("label: \"hello\""),
            "quoted attr must become string literal"
        );
        assert!(code.contains("size: 16.0"), "numeric attr must become f32");
    }

    #[test]
    fn preview_section_generates_build_fn_and_entry() {
        let src = "[logic]\n[view]\ncol\n    text \"x\"\n\n[preview \"Default\"]\ncounter\n";
        let out = transpile_source_with_theme(src, "demo", None).unwrap();
        let code = &out.rust_code;
        // A dedicated build fn per preview (so prop-taking components can be previewed)...
        assert!(
            code.contains(
                "pub fn demo_preview_0(ctx: &mut WidgetCtx) -> Result<Box<dyn LayoutItem>, LayoutError>"
            ),
            "missing preview build fn:\n{code}"
        );
        // ...whose body builds the preview's markup (here a bare component call)...
        assert!(
            code.contains("counter(ctx)?"),
            "preview body should call the component:\n{code}"
        );
        // ...and a PreviewEntry pointing at that fn, not the component fn.
        assert!(
            code.contains("build: demo_preview_0"),
            "entry should point at the preview fn:\n{code}"
        );
        assert!(code.contains("preview_name: \"Default\""));
        assert_eq!(out.preview_names, vec!["Default".to_string()]);
    }

    #[test]
    fn dollar_marks_reactive_reads_and_clones_closure_captures() {
        let src = "[logic]\nlet count = signal(0i32);\n[view]\ncol\n    text \"{$count}\"\n    btn \"+\" on_press:|| $count.update(|n| *n += 1)\n";
        let out = transpile_source_with_theme(src, "demo", None).unwrap();
        let code = &out.rust_code;
        // `$count` in interpolation is a read.
        assert!(
            code.contains("count.get()"),
            "interpolation read missing:\n{code}"
        );
        // `$count` in a closure is the handle (the `$` is stripped).
        assert!(
            code.contains("count.update(|n| *n += 1)"),
            "closure handle missing:\n{code}"
        );
        // The text and the button each clone `count`, so the two `move` closures own it independently.
        assert_eq!(
            code.matches("let count = count.clone();").count(),
            2,
            "expected one clone per capturing closure:\n{code}"
        );
        // No `$` sigil leaks into the generated Rust.
        assert!(
            !code.contains('$'),
            "the `$` marker must not reach output:\n{code}"
        );
    }

    #[test]
    fn img_src_value_carries_an_expr_span() {
        // The `src` attr is a verbatim Rust expression, so the analyzer must get an expr-span that maps back onto the `hero` identifier in source (this is what makes refs/rename precise in `[view]`).
        let src = "[logic]\nlet hero = 1i32;\n[view]\ncol\n    img src:hero width:100\n";
        let out = transpile_source_with_theme(src, "demo", None).unwrap();
        let spans: Vec<&str> = out
            .expr_spans
            .iter()
            .map(|s| &src[s.rsx_start as usize..(s.rsx_start + s.len) as usize])
            .collect();
        assert!(
            spans.contains(&"hero"),
            "img src value should map back to `hero`; got spans {spans:?}"
        );
        // And the matching span must point at byte-identical text in the generated code.
        let span = out
            .expr_spans
            .iter()
            .find(|s| &src[s.rsx_start as usize..(s.rsx_start + s.len) as usize] == "hero")
            .unwrap();
        let gs = span.gen_start as usize;
        assert_eq!(&out.rust_code[gs..gs + span.len as usize], "hero");
    }

    #[test]
    fn svg_generates_src_tint_and_layout() {
        let src =
            "[view]\ncol\n    svg src:props.icon tint:Color::hex(\"#ff5722\") width:24 height:24\n";
        let out = transpile_source_with_theme(src, "demo", None).unwrap();
        let code = &out.rust_code;
        assert!(code.contains("Svg::new("), "missing Svg::new:\n{code}");
        assert!(
            code.contains("let __src = props.icon.clone();"),
            "missing src hoist:\n{code}"
        );
        assert!(
            code.contains("move || __src.clone(),"),
            "missing src closure:\n{code}"
        );
        assert!(
            code.contains("move || Some(Color::hex(\"#ff5722\")),"),
            "missing tint closure:\n{code}"
        );
        assert!(
            code.contains(".width(24.0)") && code.contains(".height(24.0)"),
            "missing layout dims:\n{code}"
        );
    }

    #[test]
    fn svg_without_tint_generates_none() {
        let src = "[view]\ncol\n    svg src:props.icon\n";
        let out = transpile_source_with_theme(src, "demo", None).unwrap();
        assert!(
            out.rust_code.contains("|| None,"),
            "missing default tint closure:\n{}",
            out.rust_code
        );
    }

    #[test]
    fn svg_missing_src_falls_back_to_undefined_placeholder() {
        // No `src` attr: falls back to an undefined `__svg_data` identifier, so rustc's "cannot find value" error lands on this `.rsx` line via the source map — the same diagnostic strategy `img` uses for a missing `src`.
        let src = "[view]\ncol\n    svg width:24 height:24\n";
        let out = transpile_source_with_theme(src, "demo", None).unwrap();
        assert!(
            out.rust_code.contains("__svg_data"),
            "missing placeholder identifier:\n{}",
            out.rust_code
        );
    }

    #[test]
    fn svg_src_value_carries_an_expr_span() {
        let src = "[logic]\nlet icon = 1i32;\n[view]\ncol\n    svg src:icon width:24\n";
        let out = transpile_source_with_theme(src, "demo", None).unwrap();
        let spans: Vec<&str> = out
            .expr_spans
            .iter()
            .map(|s| &src[s.rsx_start as usize..(s.rsx_start + s.len) as usize])
            .collect();
        assert!(
            spans.contains(&"icon"),
            "svg src value should map back to `icon`; got spans {spans:?}"
        );
    }

    #[test]
    fn transition_opacity_hoists_animated_and_wraps_reactive_read() {
        // A `transition:opacity` over a reactive `opacity:$sig`: the Animated is hoisted into setup (built once), and the opacity closure re-targets it to the current value and reads it.
        let src = "[logic]\nlet fade = signal(1.0f32);\n[view]\nbox opacity:$fade transition:opacity 200ms ease-out\n    text \"hi\"\n";
        let code = transpile_source_with_theme(src, "demo", None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains("let __transition_0 = motion::Animated::new(fade.get(), motion::tween(std::time::Duration::from_millis(200), motion::Easing::EaseOut));"),
            "missing hoisted Animated:\n{code}"
        );
        assert!(
            code.contains(".with_opacity({ let fade = fade.clone(); move || { __transition_0.retarget(fade.get()); __transition_0.get() } })"),
            "missing opacity retarget+get:\n{code}"
        );
    }

    #[test]
    fn transition_fill_with_theme_color_and_cubic_bezier() {
        let src = "[view]\nbox fill:primary transition:fill 150ms cubic-bezier(0.4,0,0.2,1)\n    text \"x\"\n";
        let code = transpile_source_with_theme(src, "demo", Some("SandboxTheme"))
            .unwrap()
            .rust_code;
        assert!(
            code.contains("let __transition_0 = motion::Animated::new(use_theme::<SandboxTheme>().primary, motion::tween(std::time::Duration::from_millis(150), motion::Easing::CubicBezier(0.4, 0.0, 0.2, 1.0)));"),
            "missing cubic-bezier Animated:\n{code}"
        );
        assert!(
            code.contains(".with_fill({ __transition_0.retarget(use_theme::<SandboxTheme>().primary); __transition_0.get() })"),
            "missing fill retarget+get:\n{code}"
        );
    }

    #[test]
    fn transition_fill_spring() {
        let src = "[view]\nbox fill:#3d78fa transition:fill spring(170,26)\n";
        let code = transpile_source_with_theme(src, "demo", None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains("motion::spring(170.0, 26.0)"),
            "missing spring curve:\n{code}"
        );
        assert!(
            code.contains("motion::Animated::new(Color::rgba(61.0 / 255.0, 120.0 / 255.0, 250.0 / 255.0, 255.0 / 255.0), motion::spring(170.0, 26.0))"),
            "spring Animated should seed from the fill color:\n{code}"
        );
    }

    #[test]
    fn transition_multiple_properties_comma_separated() {
        let src = "[logic]\nlet fade = signal(1.0f32);\n[view]\nbox fill:#3d78fa opacity:$fade transition:opacity 200ms, fill 150ms linear\n";
        let code = transpile_source_with_theme(src, "demo", None)
            .unwrap()
            .rust_code;
        // The fill transition uses linear...
        assert!(
            code.contains(
                "motion::tween(std::time::Duration::from_millis(150), motion::Easing::Linear)"
            ),
            "missing fill linear tween:\n{code}"
        );
        // ...and the opacity transition uses the default ease-out (no easing given).
        assert!(
            code.contains(
                "motion::tween(std::time::Duration::from_millis(200), motion::Easing::EaseOut)"
            ),
            "missing opacity default-easing tween:\n{code}"
        );
        // Two distinct Animated handles are hoisted.
        assert!(
            code.contains("let __transition_0 =") && code.contains("let __transition_1 ="),
            "expected two hoisted animations:\n{code}"
        );
    }

    #[test]
    fn transition_unsupported_property_emits_compile_error() {
        let src = "[view]\nbox fill:#3d78fa transition:radius 200ms\n";
        let code = transpile_source_with_theme(src, "demo", None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains("compile_error!(\"transition: unsupported property `radius`"),
            "unsupported prop should emit a compile_error:\n{code}"
        );
    }

    #[test]
    fn transition_invalid_duration_emits_compile_error() {
        let src = "[view]\nbox opacity:0.5 transition:opacity 200\n";
        let code = transpile_source_with_theme(src, "demo", None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains("compile_error!(\"transition:opacity has an invalid duration `200`"),
            "invalid duration should emit a compile_error:\n{code}"
        );
    }

    #[test]
    fn transition_inside_for_loop_hoists_animated_per_iteration() {
        // `for` is a construction loop (runs once per component instance, pushing one widget per item into `__children`), not a reactive list needing key-based identity; the `Animated` for a `transition:` inside its body must sit inside the loop's own per-iteration `let __sbox_N = { .. }` block, so a fresh, persistent handle is installed for every item.
        let src = "[logic]\nlet items = vec![1,2,3];\n[view]\ncol\n    for item in items.iter()\n        box fill:#3d78fa transition:fill 200ms\n";
        let code = transpile_source_with_theme(src, "demo", None)
            .unwrap()
            .rust_code;
        assert!(
            !code.contains("compile_error!"),
            "transition inside a for must be accepted:\n{code}"
        );
        assert!(
            code.contains("let __transition_0 = motion::Animated::new(Color::rgba(61.0 / 255.0, 120.0 / 255.0, 250.0 / 255.0, 255.0 / 255.0), motion::tween(std::time::Duration::from_millis(200), motion::Easing::EaseOut));"),
            "missing hoisted Animated:\n{code}"
        );
        // The hoist must be textually nested inside the `for` body (between the loop header and its own widget's `StyledContainer::new`), so it re-installs once per iteration at runtime.
        let for_pos = code
            .find("for item in items.iter() {")
            .expect("for loop emitted");
        let hoist_pos = code
            .find("let __transition_0 =")
            .expect("hoisted Animated present");
        let ctor_pos = code
            .find("StyledContainer::new")
            .expect("styled container emitted");
        assert!(
            for_pos < hoist_pos && hoist_pos < ctor_pos,
            "Animated hoist must sit inside the loop body, before its widget's constructor:\n{code}"
        );
    }

    #[test]
    fn transition_inside_for_loop_uses_distinct_counters_per_element() {
        // Two elements with `transition:` in the same loop body are two distinct code sites, so the global `transition_count` must still hand out unique names for each — not one shared name reused per iteration.
        let src = "[logic]\nlet items = vec![1,2,3];\n[view]\ncol\n    for item in items.iter()\n        box fill:#3d78fa transition:fill 150ms\n        box stroke:#111111 transition:stroke 150ms\n";
        let code = transpile_source_with_theme(src, "demo", None)
            .unwrap()
            .rust_code;
        assert!(
            !code.contains("compile_error!"),
            "both transitions inside the for must be accepted:\n{code}"
        );
        assert!(
            code.contains("let __transition_0 =") && code.contains("let __transition_1 ="),
            "expected two distinct hoisted animations, one per element in the loop body:\n{code}"
        );
    }

    #[test]
    fn reactive_opacity_without_transition_reads_signal_each_run() {
        let src = "[logic]\nlet fade = signal(0.5f32);\n[view]\nbox opacity:$fade\n";
        let code = transpile_source_with_theme(src, "demo", None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains(".with_opacity({ let fade = fade.clone(); move || fade.get() })"),
            "reactive opacity should read the signal in a closure:\n{code}"
        );
        assert!(
            !code.contains("motion::"),
            "no transition means no motion usage:\n{code}"
        );
    }

    #[test]
    fn static_opacity_still_supported_as_closure() {
        // T-3.1: opacity is now a closure; a static value becomes a capture-free `|| 0.5`.
        let src = "[view]\nbox fill:#3d78fa opacity:0.5\n";
        let code = transpile_source_with_theme(src, "demo", None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains(".with_opacity(|| 0.5)"),
            "static opacity should emit a capture-free closure:\n{code}"
        );
    }

    #[test]
    fn transition_fill_from_class_is_wired_without_false_error() {
        // The fill comes from the `@card` class, not an inline attribute; it must still be animatable (no spurious "no matching value").
        let src = "[style]\n@card\n    fill: #3d78fa\n    radius: 12\n[view]\ncol @card transition:fill 150ms\n    text \"x\"\n";
        let code = transpile_source_with_theme(src, "demo", None)
            .unwrap()
            .rust_code;
        assert!(
            !code.contains("compile_error!"),
            "class-provided fill should be animatable:\n{code}"
        );
        assert!(
            code.contains("motion::Animated::new"),
            "class fill transition should be wired:\n{code}"
        );
    }

    #[test]
    fn transition_color_on_text_wraps_text_style() {
        let src = "[view]\ntext \"hi\" color:primary transition:color 120ms\n";
        let code = transpile_source_with_theme(src, "demo", Some("SandboxTheme"))
            .unwrap()
            .rust_code;
        assert!(
            code.contains("let __transition_0 = motion::Animated::new(use_theme::<SandboxTheme>().primary, motion::tween(std::time::Duration::from_millis(120), motion::Easing::EaseOut));"),
            "missing hoisted color Animated:\n{code}"
        );
        assert!(
            code.contains("TextStyle::new(14.0, { __transition_0.retarget(use_theme::<SandboxTheme>().primary); __transition_0.get() })"),
            "text color should be wrapped in the transition block:\n{code}"
        );
    }

    #[test]
    fn fill_signal_reads_reactively_and_clones_into_the_closure() {
        // No `transition:`: `fill:$accent` must still re-evaluate every time the styling closure runs, and must clone `accent` into that closure so the outer binding (declared in `[logic]`) stays usable elsewhere.
        let src = "[logic]\nlet accent = signal(Color::WHITE);\n[view]\nbox fill:$accent\n";
        let code = transpile_source_with_theme(src, "demo", None)
            .unwrap()
            .rust_code;
        assert!(
            !code.contains("compile_error!"),
            "a signal fill must not error:\n{code}"
        );
        assert!(
            code.contains("{ let accent = accent.clone(); move |_| RectStyle::default().with_fill(accent.get()).with_radius(BorderRadius::zero()) }"),
            "fill should reactively read the cloned signal:\n{code}"
        );
    }

    #[test]
    fn fill_signal_with_spring_transition_seeds_and_retargets_from_the_same_read() {
        // The `Animated`'s initial value and every `retarget` call must both read through the same `accent.get()` expression — the transition mechanism wraps a `$signal` fill exactly like it already does theme colors.
        let src = "[logic]\nlet accent = signal(Color::WHITE);\n[view]\nbox fill:$accent transition:fill spring(170, 26)\n";
        let code = transpile_source_with_theme(src, "demo", None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains(
                "let __transition_0 = motion::Animated::new(accent.get(), motion::spring(170.0, 26.0));"
            ),
            "missing hoisted Animated seeded from accent.get():\n{code}"
        );
        assert!(
            code.contains(
                "{ let accent = accent.clone(); move |_| RectStyle::default().with_fill({ __transition_0.retarget(accent.get()); __transition_0.get() }).with_radius(BorderRadius::zero()) }"
            ),
            "fill retarget+get should read accent.get() through the cloned signal:\n{code}"
        );
    }

    #[test]
    fn stroke_signal_reads_reactively_and_clones_into_the_closure() {
        // `stroke:` shares `color_expr`/`rect_style_pieces` with `fill:`, so `$ident` must work identically.
        let src = "[logic]\nlet accent = signal(Color::WHITE);\n[view]\nbox stroke:$accent\n";
        let code = transpile_source_with_theme(src, "demo", None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains(
                "{ let accent = accent.clone(); move |_| RectStyle { fill: None, stroke: Some(Stroke::new(accent.get(), 1.0)), shadow: None, radius: BorderRadius::zero() } }"
            ),
            "stroke should reactively read the cloned signal:\n{code}"
        );
    }

    #[test]
    fn text_color_signal_reads_reactively_and_clones_into_the_closure() {
        // `text`'s `color:` also shares `color_expr` (via `text_style`), confirming the `$ident` branch and its clone-wrapping generalize beyond fill/stroke.
        let src =
            "[logic]\nlet accent = signal(Color::WHITE);\n[view]\ntext \"hi\" color:$accent\n";
        let code = transpile_source_with_theme(src, "demo", None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains(
                "{ let accent = accent.clone(); move || TextStyle::new(14.0, accent.get()) }"
            ),
            "text color should reactively read the cloned signal:\n{code}"
        );
    }

    #[test]
    fn hex_theme_and_keyword_colors_are_unaffected_by_signal_support() {
        // Regression guard: adding the `$ident` branch to `color_expr` must not touch the pre-existing hex/theme/keyword paths.
        let src = "[view]\nbox fill:#3d78fa stroke:white\n    text \"x\" color:primary\n";
        let code = transpile_source_with_theme(src, "demo", Some("SandboxTheme"))
            .unwrap()
            .rust_code;
        assert!(
            code.contains("Color::rgba(61.0 / 255.0, 120.0 / 255.0, 250.0 / 255.0, 255.0 / 255.0)")
        );
        assert!(code.contains("Color::WHITE"));
        assert!(code.contains("use_theme::<SandboxTheme>().primary"));
        assert!(
            !code.contains(".clone()"),
            "no signal clone should appear for static/theme colors:\n{code}"
        );
    }
}
