//! RSX transpiler: converts a parsed [`RsxDocument`] AST into compilable Rust
//! source code that depends on `rsx::*`.

mod error;
pub mod naming;
mod preview_scan;
mod registry;
mod signal_scan;
mod style;
mod view;

pub use error::TranspileError;
pub use registry::{TAG_REFERENCES_VARIABLE, builtin_tags, layout_attr_keys};

use std::path::{Path, PathBuf};

use rsx_parser::RsxDocument;

use crate::naming::{
    contains_ident, preview_entries_const_name, replace_whole_word, to_pascal_case, to_snake_case,
};
use crate::preview_scan::scan_previews;
use crate::signal_scan::scan_signals;
use crate::style::generate_style_section;
use crate::view::ViewGen;

/// Input to a single transpilation: the parsed document plus the desired
/// component function name (typically derived from the source file stem).
pub(crate) struct TranspileInput<'a> {
    pub document: &'a RsxDocument,
    pub component_name: &'a str,
    /// Concrete theme type path (e.g. `SandboxTheme`). When set, `[style]` color
    /// references resolve through `use_theme::<Type>()` instead of `COLOR_*` consts.
    pub theme_type: Option<&'a str>,
}

/// The generated Rust source for one `.rsx` file.
pub struct TranspiledSource {
    pub rust_code: String,
    pub preview_names: Vec<String>,
    /// Per generated line (0-based), the 0-based `.rsx` line it originated from, or `None` for
    /// boilerplate and transpiler-injected lines. Lets the analyzer map rust-analyzer's diagnostics
    /// on the generated code back onto the `.rsx` source.
    pub source_map: Vec<Option<u32>>,
}

/// Serializes a [`TranspiledSource::source_map`] as a JSON array (`[null,3,3,...]`), the format the
/// editor extension reads to map rust-analyzer's diagnostics on the generated Rust back to `.rsx`.
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

/// Parses `source` and generates Rust for `component_name`, resolving `[style]` colors
/// through `theme_type` when provided so theme switching at runtime takes effect.
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

/// Recursively collects files with `extension` under `dir`, descending into a
/// subdirectory only when `keep_dir` returns true for its name. The result is sorted.
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

/// Derives a unique stem for a `.rsx` file from its path relative to `src_dir`,
/// flattening subdirectories with `_` so files in different directories don't
/// collide (e.g. `src/components/button.rsx` -> `components_button`).
pub fn relative_stem(path: &Path, src_dir: &Path) -> String {
    let rel = path.strip_prefix(src_dir).unwrap_or(path);
    let without_ext = rel.with_extension("");
    without_ext
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("_")
}

/// Derives the output `.rs` path (relative to the output root) for a `.rsx`
/// file by mirroring its location under `src_dir`, so files in different
/// directories never collide (e.g. `src/components/button.rsx` ->
/// `components/button.rs`). Used for the transpiler's `.rsx/build/` output.
/// Returns `None` for files outside `src_dir`: those are never transpiled, so
/// they have no place in the build tree — and flattening their absolute path
/// would escape the output root entirely.
pub fn relative_output_path(path: &Path, src_dir: &Path) -> Option<PathBuf> {
    let rel = path.strip_prefix(src_dir).ok()?;
    if rel.as_os_str().is_empty() {
        return None;
    }
    Some(rel.with_extension("rs"))
}

/// Accumulates generated code together with a per-line origin map. Each completed line (terminated by
/// `\n`) records the `.rsx` source line passed when its newline was appended, so callers tag a line by
/// emitting its content and the closing newline with the same `src`.
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
    let previews = scan_previews(logic_source);

    let style_section = generate_style_section(&doc.style, input.theme_type.is_some());

    let mut view_gen = ViewGen::with_theme(
        &signals,
        &doc.style.classes,
        &doc.style.constants,
        input.theme_type,
    );
    let view_body = view_gen.generate_root(&doc.view.nodes);
    let uses_theme = view_gen.uses_theme();

    let logic = logic_source.trim_end();

    let extra_params: Vec<String> = doc
        .props
        .parameters
        .iter()
        .map(|p| format!("{}: {}", p.name, p.ty))
        .collect();
    let extra_params_str = if extra_params.is_empty() {
        String::new()
    } else {
        format!(", {}", extra_params.join(", "))
    };

    let signature = if has_props {
        format!(
            "pub fn {fn_name}(ctx: &mut WidgetCtx, props: {props_type}{extra_params_str}) -> Result<Box<dyn LayoutItem>, LayoutError>"
        )
    } else {
        format!(
            "pub fn {fn_name}(ctx: &mut WidgetCtx{extra_params_str}) -> Result<Box<dyn LayoutItem>, LayoutError>"
        )
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
    // Outer attribute on use so this file is safe to include! from another crate.
    code.push("#[allow(unused_imports)] use rsx::*;\n", None);
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
        let mut declared: Vec<&str> = Vec::new();
        for (j, line) in logic.lines().enumerate() {
            let src = Some(logic_line_src(j));
            if line.is_empty() {
                code.push("\n", src);
                continue;
            }
            // Preview attributes are metadata for the bundler, not Rust code; strip them.
            let trimmed_line = line.trim();
            if trimmed_line.starts_with("#[preview(") || trimmed_line == "#[preview]" {
                continue;
            }
            // If this line has a `move` closure that captures a previously declared signal, emit a dedicated clone with a mangled name for the closure, then rewrite the line so the closure captures that clone instead of the original. This leaves the original binding intact for the view code.
            let mut emitted_line = line.to_string();
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

    // The view body carries source markers from generation; resolve them into per-line origins so
    // diagnostics on the generated view map back to the `.rsx` element they came from.
    for (line, src) in crate::view::resolve_source_map(&view_body) {
        code.push(&line, src);
        code.push("\n", src);
    }
    if !code.out.ends_with('\n') {
        code.push("\n", None);
    }
    code.push("}\n", None);

    if !previews.is_empty() {
        code.push("\n", None);
        let const_name = preview_entries_const_name(&fn_name);
        code.push(
            &format!("pub const {const_name}: &[::rsx::PreviewEntry] = &[\n"),
            None,
        );
        for preview in &previews {
            code.push(
                &format!(
                    "    ::rsx::PreviewEntry {{ component_name: \"{fn_name}\", preview_name: \"{}\", build: {fn_name} }},\n",
                    preview.name.replace('"', "\\\"")
                ),
                None,
            );
        }
        code.push("];\n", None);
    }

    Ok(TranspiledSource {
        rust_code: code.out,
        preview_names: previews.iter().map(|p| p.name.clone()).collect(),
        source_map: code.map,
    })
}

/// Extracts `pub struct Props { … }` (plus any preceding `#[…]` attribute lines)
/// from the logic zone, renames it to `{PascalFnName}Props`, and returns the
/// renamed struct code together with the logic zone with the struct removed.
///
/// Returns `(None, original_logic, None)` when no `struct Props` is found; otherwise the third
/// element is the `[start, end]` (inclusive) line span of the struct within `logic`'s lines, so the
/// caller can map the emitted struct back to the original source.
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
        let src = "[logic]\n#[derive(Props)]\npub struct Props {\n    pub body: &'static st,\n}\nlet count = create_rw_signal(0i32);\n\n[view]\ncol\n";
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
            .position(|l| l.contains("create_rw_signal"))
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

        // A container's own closing constructor maps back to the container, not its last child:
        // the row's `Container::new` resolves to the `row` line (4 -> 3) even though the btn nested inside.
        let row_ctor = lines
            .iter()
            .position(|l| l.contains("flex_row()"))
            .expect("generated row container");
        assert_eq!(result.source_map[row_ctor], Some(3));
    }

    // COUNTER declares [style] colors: with no theme they become local COLOR_* consts; with a theme_type they resolve through use_theme instead (see the theme tests below).
    const COUNTER: &str = r#"[logic]
let count = create_rw_signal(0i32);

[style]
primary: #3d78fa
dark: #141424

.card
    width: 240
    padding: 20
    gap: 12
    direction: col
    align: center

[view]
col .card
    text "Count: {count}" size:14 color:dark
    btn "Increment" fill:primary on_press:|| count.update(|n| *n += 1)
"#;

    // COUNTER_THEMED has no [style] color declarations — colors flow through the live theme so they react to `set_theme_with_widgets(...)` calls at runtime.
    const COUNTER_THEMED: &str = r#"[logic]
let count = create_rw_signal(0i32);

[style]
.card
    width: 240
    padding: 20
    gap: 12
    direction: col
    align: center

[view]
col .card
    text "Count: {count}" size:14 color:dark
    btn "Increment" fill:primary on_press:|| count.update(|n| *n += 1)
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
        assert!(code.contains("move || format!(\"Count: {}\", count.get())"));
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
}
