//! Generates the body of the component function from the `[view]` section.

mod button;
mod canvas;
mod component;
mod container;
mod control_flow;
mod interp;
mod media;
mod scroll;
mod signals;
mod style_helpers;
mod text;

use std::collections::HashMap;
use std::fmt::Write;
use std::path::{Path, PathBuf};

use rsx_parser::{Element, StyleClass, StyleConstant, ViewNode};

/// Sentinel comment lines that bracket each view node's generated code with the `.rsx` line it came from. They are emitted into the view body during generation and stripped by [`resolve_source_map`] in the transpiler, which turns them into the per-line origin map. The prefix is deliberately un-generatable by normal codegen so it can never collide with real output.
const SRC_PUSH: &str = "//@RSX@PUSH:";
const SRC_POP: &str = "//@RSX@POP";

/// Inline marker emitted immediately before a verbatim `[view]` Rust expression (interpolation `{expr}`, `if`/`let` expressions, closure attr values). Stripped by [`resolve_source_map`], which records the byte position where the expression begins in the generated body, so the analyzer can map a `.rsx` cursor inside the expression onto the generated Rust. Payload: `<rsx_start>:<len>`, the source byte offset and length of the fragment, which is byte-identical in source and output.
const SRC_EXPR_OPEN: &str = "/*@RSX@EXPR:";
const SRC_EXPR_CLOSE: &str = "@*/";

/// Builds an [`SRC_EXPR_OPEN`] marker for a verbatim expression at source byte offset `rsx_start` spanning `len` bytes.
fn expr_marker(rsx_start: usize, len: usize) -> String {
    format!("{SRC_EXPR_OPEN}{rsx_start}:{len}{SRC_EXPR_CLOSE}")
}

/// Wraps `emit`'s code in `SRC_PUSH`/`SRC_POP` markers carrying the 0-based `.rsx` `line`.
fn wrap_source_markers(emit: ChildEmit, line: usize) -> ChildEmit {
    let src0 = line.saturating_sub(1);
    let wrap = |code: String| format!("{SRC_PUSH}{src0}\n{code}\n{SRC_POP}");
    match emit {
        ChildEmit::Simple { name, code } => ChildEmit::Simple {
            name,
            code: wrap(code),
        },
        ChildEmit::Dynamic { code } => ChildEmit::Dynamic { code: wrap(code) },
    }
}

/// The resolved view body: real lines (each with its origin `.rsx` line) plus the byte spans of the verbatim Rust expressions it contains.
pub(crate) struct ResolvedView {
    pub lines: Vec<(String, Option<u32>)>,
    /// Per expression: `(byte offset within the streamed body, rsx_start, len)`. The streamed body is the lines joined with `\n` (each line followed by a newline), matching how `transpile` appends them, so a caller adds the body's start offset in the final file to get the generated offset.
    pub expr_spans: Vec<(usize, u32, u32)>,
}

/// Strips the source markers from a generated view body, returning each real line paired with the `.rsx` line it originated from (a stack tracks nesting, so a node's own lines map to itself and its children's lines map to the children) plus the verbatim-expression byte spans. Lines outside any marker (root boilerplate) map to `None`.
pub(crate) fn resolve_source_map(marked: &str) -> ResolvedView {
    let mut stack: Vec<u32> = Vec::new();
    let mut lines = Vec::new();
    let mut expr_spans = Vec::new();
    // Running byte length of the streamed body (each emitted line plus its trailing `\n`).
    let mut body_len = 0usize;
    for line in marked.split('\n') {
        if let Some(rest) = line.strip_prefix(SRC_PUSH) {
            if let Ok(n) = rest.parse::<u32>() {
                stack.push(n);
            }
        } else if line == SRC_POP {
            stack.pop();
        } else {
            let (clean, spans) = strip_expr_markers(line, body_len);
            expr_spans.extend(spans);
            body_len += clean.len() + 1;
            lines.push((clean, stack.last().copied()));
        }
    }
    ResolvedView { lines, expr_spans }
}

/// Removes inline [`SRC_EXPR_OPEN`] markers from a single output `line`, returning the cleaned line and, for each marker, `(body offset of the following expression, rsx_start, len)`. `base` is the body byte offset of this line's start; the expression begins exactly where the marker was, so its offset is `base + <cleaned bytes emitted so far>`.
fn strip_expr_markers(line: &str, base: usize) -> (String, Vec<(usize, u32, u32)>) {
    let mut out = String::with_capacity(line.len());
    let mut spans = Vec::new();
    let mut rest = line;
    while let Some(open) = rest.find(SRC_EXPR_OPEN) {
        out.push_str(&rest[..open]);
        let after_open = &rest[open + SRC_EXPR_OPEN.len()..];
        let Some(close) = after_open.find(SRC_EXPR_CLOSE) else {
            // Malformed marker (no close): keep the remainder verbatim and stop.
            out.push_str(rest);
            return (out, spans);
        };
        let payload = &after_open[..close];
        if let Some((rsx_start, len)) = payload.split_once(':')
            && let (Ok(rsx_start), Ok(len)) = (rsx_start.parse::<u32>(), len.parse::<u32>())
        {
            spans.push((base + out.len(), rsx_start, len));
        }
        rest = &after_open[close + SRC_EXPR_CLOSE.len()..];
    }
    out.push_str(rest);
    (out, spans)
}

/// A piece of generated child code together with how it contributes to a parent's child collection.
pub(crate) enum ChildEmit {
    /// A simple widget bound to `name`, pushable directly.
    Simple { name: String, code: String },
    /// Control flow (`if`/`for`) that mutates a child vector in place.
    Dynamic { code: String },
}

pub struct ViewGen<'a> {
    /// Declared style classes, used to validate class references in elements.
    classes: &'a [StyleClass],
    constants: &'a [StyleConstant],
    /// Per-widget-type variable counters, keyed by the descriptive prefix.
    counters: HashMap<String, usize>,
    /// When set, `[style]` color references resolve to `use_theme::<Type>().field` instead of generated `COLOR_*` consts, so theme switching takes effect.
    theme_type: Option<String>,
    /// Indentation depth (in 4-space units) for the current emission scope.
    indent: usize,
    /// Loop-variable identifiers currently in scope, cloned per closure like signals.
    loop_variables: Vec<String>,
    /// Monotonic counter for the hoisted `__transition_N` animation handles.
    transition_count: usize,
    /// Directory of the `.rsx` being transpiled, used to resolve static `svg`/`img` asset paths for build-time baking. `None` (e.g. an in-memory transpile) makes a static `src:"path"` yield a `compile_error!`.
    base_dir: Option<PathBuf>,
    /// Monotonic counter for the hoisted `BAKED_*_N` static asset handles, unique per component so two baked assets never share a `static` name.
    baked_asset_count: usize,
}

impl<'a> ViewGen<'a> {
    pub fn with_theme(
        classes: &'a [StyleClass],
        constants: &'a [StyleConstant],
        theme_type: Option<&str>,
        base_dir: Option<&Path>,
    ) -> Self {
        Self {
            classes,
            constants,
            counters: HashMap::new(),
            theme_type: theme_type.map(str::to_string),
            indent: 1,
            loop_variables: Vec::new(),
            transition_count: 0,
            base_dir: base_dir.map(Path::to_path_buf),
            baked_asset_count: 0,
        }
    }

    fn next_variable_name(&mut self, tag: &str) -> String {
        let prefix = match tag {
            "text" => "text",
            "btn" | "button" => "btn",
            "col" | "column" => "col",
            "row" => "row",
            "box" => "sbox",
            "img" | "image" => "img",
            "svg" => "svg",
            "canvas" => "canvas",
            _ => "node",
        };
        let count = self.counters.entry(prefix.to_string()).or_insert(0);
        let name = format!("__{prefix}_{count}");
        *count += 1;
        name
    }

    fn indent_str(&self) -> String {
        "    ".repeat(self.indent)
    }

    /// Generates the full view body and returns the final `Ok(Box::new(...))` expression.
    pub fn generate_root(&mut self, nodes: &[ViewNode]) -> String {
        let mut out = String::new();
        let mut last_widget: Option<String> = None;
        let mut roots = Vec::new();

        for node in nodes {
            match self.emit_node(node) {
                ChildEmit::Simple { name, code } => {
                    out.push_str(&code);
                    out.push('\n');
                    last_widget = Some(name.clone());
                    roots.push(name);
                }
                ChildEmit::Dynamic { code } => {
                    // A bare control-flow node at the root has no container to attach to; emit it verbatim for completeness.
                    out.push_str(&code);
                    out.push('\n');
                }
            }
        }

        let pad = self.indent_str();
        match roots.len() {
            0 => {
                let _ = write!(
                    out,
                    "{pad}Ok(Box::new(Container::column(ctx, children![])?))"
                );
            }
            1 => {
                let only = last_widget.unwrap_or_else(|| roots[0].clone());
                let _ = write!(out, "{pad}Ok(Box::new({only}))");
            }
            _ => {
                let items = roots.join(", ");
                let _ = write!(
                    out,
                    "{pad}Ok(Box::new(Container::column(ctx, children![{items}])?))"
                );
            }
        }

        out
    }

    fn emit_node(&mut self, node: &ViewNode) -> ChildEmit {
        let emit = match node {
            ViewNode::Element(el) => self.emit_element(el),
            ViewNode::LetStmt {
                source,
                source_start,
            } => ChildEmit::Dynamic {
                code: format!(
                    "{}{}{source};",
                    self.indent_str(),
                    expr_marker(*source_start, source.len())
                ),
            },
            ViewNode::IfBlock(block) => self.emit_if(block),
            ViewNode::ForBlock(block) => self.emit_for(block),
        };
        // Bracket this node's generated lines with source markers so the transpiler can map them back to the `.rsx` line. Nested nodes nest their own markers; `let` statements have no line of their own and inherit the enclosing node's mapping.
        match node {
            ViewNode::Element(el) => wrap_source_markers(emit, el.line),
            ViewNode::IfBlock(block) => wrap_source_markers(emit, block.line),
            ViewNode::ForBlock(block) => wrap_source_markers(emit, block.line),
            ViewNode::LetStmt { .. } => emit,
        }
    }

    fn emit_element(&mut self, el: &Element) -> ChildEmit {
        match el.tag.as_str() {
            "text" => self.emit_text(el),
            "heading" => self.emit_heading(el),
            "section" => self.emit_section(el),
            "btn" => self.emit_button(el),
            "col" | "row" | "grid" => self.emit_container(el),
            "box" => self.emit_box(el),
            "img" | "image" => self.emit_image(el),
            "svg" => self.emit_svg(el),
            "scroll" => self.emit_scroll(el),
            "canvas" => self.emit_canvas(el),
            "widget" => self.emit_widget_ref(el),
            other => self.emit_component_call(el, other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::signals::normalize_closure;
    use super::*;

    fn make_gen<'a>() -> ViewGen<'a> {
        ViewGen::with_theme(&[], &[], None, None)
    }

    #[test]
    fn literal_content() {
        let g = make_gen();
        assert_eq!(
            g.interpolate_content("hello", 0),
            "|| \"hello\".to_string()"
        );
    }

    #[test]
    fn signal_interpolation() {
        let g = make_gen();
        // `$count` is a reactive read; the rewritten expression carries no verbatim span.
        assert_eq!(
            g.interpolate_content("Count: {$count}", 0),
            "move || format!(\"Count: {}\", { count.get() })"
        );
    }

    #[test]
    fn closure_passthrough() {
        assert_eq!(
            normalize_closure("|| count.update(|n| *n += 1)"),
            "|| count.update(|n| *n += 1)"
        );
    }

    #[test]
    fn widget_ref_passthrough() {
        let src = "[logic]\nlet canvas = build_canvas(ctx)?;\n[view]\nwidget \"canvas\"\n";
        let out = crate::transpile_source_with_theme(src, "my_section", None, None).unwrap();
        assert!(out.rust_code.contains("Ok(Box::new(canvas))"));
    }

    #[test]
    fn canvas_with_rect_and_text_children() {
        let src = "[logic]\n[view]\ncanvas width:100 height:50\n    rect fill:#3c77fa radius:8\n    text \"hi\" x:0 y:4 w:full h:42 size:12 color:white\n";
        let out = crate::transpile_source_with_theme(src, "demo", None, None).unwrap();
        let code = &out.rust_code;
        assert!(code.contains("Canvas::new(ctx,"), "missing Canvas::new");
        assert!(
            code.contains("let __w = __rect.width;"),
            "missing __w binding"
        );
        assert!(
            code.contains("let __h = __rect.height;"),
            "missing __h binding"
        );
        assert!(
            code.contains("RenderNode::rect("),
            "missing RenderNode::rect"
        );
        assert!(
            code.contains("RenderNode::group(["),
            "missing RenderNode::group"
        );
        assert!(
            code.contains("RenderNode::text("),
            "missing RenderNode::text"
        );
        assert!(code.contains("__w"), "w:full should resolve to __w");
        assert!(
            code.contains("Color::WHITE"),
            "color:white should resolve to Color::WHITE"
        );
    }

    #[test]
    fn unknown_tag_becomes_component_call() {
        let src = "[logic]\n[view]\nmy_card\n";
        let out = crate::transpile_source_with_theme(src, "demo", None, None).unwrap();
        assert!(
            out.rust_code.contains("my_card(ctx)?"),
            "no-attr tag should call fn directly"
        );
    }

    #[test]
    fn class_paint_promotes_container_and_is_consumed() {
        let src = "[style]\n@card\n    fill: #ffffff\n    radius: 12\n    padding: 8\n[view]\ncol @card\n    text \"hi\"\n";
        let out = crate::transpile_source_with_theme(src, "demo", None, None).unwrap();
        let code = &out.rust_code;
        // A `col` carrying paint from its class becomes a StyledContainer, not a plain Container.
        assert!(
            code.contains("StyledContainer::new(ctx, style_card()"),
            "painted col should be a StyledContainer:\n{code}"
        );
        // The class's fill reaches the RectStyle.
        assert!(
            code.contains("with_fill"),
            "class fill should reach the RectStyle:\n{code}"
        );
    }
}
