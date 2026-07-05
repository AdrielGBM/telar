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
    /// Signatures of every component in the workspace, so `emit_component_call` emits optional props and the slot argument correctly. `None` falls back to the per-file heuristic.
    registry: Option<&'a crate::codegen::ComponentRegistry>,
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
            registry: None,
        }
    }

    /// Attaches the workspace component registry so `emit_component_call` can consult callee signatures.
    pub(crate) fn with_registry(
        mut self,
        registry: Option<&'a crate::codegen::ComponentRegistry>,
    ) -> Self {
        self.registry = registry;
        self
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
            "children" => self.emit_slot(el),
            other => self.emit_component_call(el, other),
        }
    }
}

/// Whether a view node must build a mutable `__children` vec rather than a `children![...]` literal:
/// control flow (`if`/`for`/`let`) that mutates the vec in place, or a `children` slot placeholder that
/// splices a runtime `Vec` into it.
pub(crate) fn forces_child_vec(node: &ViewNode) -> bool {
    match node {
        ViewNode::IfBlock(_) | ViewNode::ForBlock(_) | ViewNode::LetStmt { .. } => true,
        ViewNode::Element(el) => el.tag == "children",
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

    #[test]
    fn container_on_press_emits_click_handler() {
        // A painted `box` (StyledContainer) and a plain `col` (Container) both wire `.on_press`.
        let src = "[logic]\nlet n = signal(0i32);\n[view]\ncol\n    box fill:primary on_press:|| $n.update(|v| *v += 1)\n    col on_press:|| $n.set(0)\n        text \"x\"\n";
        let out = crate::transpile_source_with_theme(src, "demo", None, None).unwrap();
        let code = &out.rust_code;
        assert!(
            code.matches(".on_press(").count() >= 2,
            "both box and col should emit .on_press:\n{code}"
        );
        // The signal is cloned into the closure so the outer handle stays usable elsewhere.
        assert!(
            code.contains("let n = n.clone();"),
            "on_press closure should clone the captured signal:\n{code}"
        );
        // `$n` is rewritten to the bare handle inside the closure body.
        assert!(
            code.contains("n.update(") && code.contains("n.set(0)"),
            "$n should be substituted to the handle:\n{code}"
        );
    }

    #[test]
    fn compound_assign_sugar_rewrites_to_update() {
        let src = "[logic]\nlet count = signal(0i32);\n[view]\ncol\n    btn \"+\" on_press:|| $count += 1\n    btn \"-\" on_press:|| $count -= 2\n";
        let out = crate::transpile_source_with_theme(src, "demo", None, None).unwrap();
        let code = &out.rust_code;
        assert!(
            code.contains("count.update(|__v| *__v += 1)"),
            "+= should desugar to update:\n{code}"
        );
        assert!(
            code.contains("count.update(|__v| *__v -= 2)"),
            "-= should desugar to update:\n{code}"
        );
    }

    #[test]
    fn toggle_and_update_closures_pass_through() {
        // `.toggle()` (a real RwSignal<bool> method) and an explicit `.update(...)` are left untouched.
        let src = "[logic]\nlet flag = signal(false);\nlet count = signal(0i32);\n[view]\ncol\n    btn \"t\" on_press:|| $flag.toggle()\n    btn \"u\" on_press:|| $count.update(|n| *n += 1)\n";
        let out = crate::transpile_source_with_theme(src, "demo", None, None).unwrap();
        let code = &out.rust_code;
        assert!(
            code.contains("flag.toggle()"),
            "toggle passes through:\n{code}"
        );
        assert!(
            code.contains("count.update(|n| *n += 1)"),
            ".update must be left untouched:\n{code}"
        );
    }

    #[test]
    fn quoted_escape_decodes_then_reemits() {
        // `\"` in .rsx content decodes to a real quote, then re-emits as an escaped quote in the Rust literal.
        let src = "[logic]\n[view]\ntext \"say \\\"hi\\\"\"\n";
        let out = crate::transpile_source_with_theme(src, "demo", None, None).unwrap();
        let code = &out.rust_code;
        assert!(
            code.contains(r#"say \"hi\""#),
            "escaped quotes should round-trip:\n{code}"
        );
    }

    #[test]
    fn paren_attr_form_captures_nested_and_coexists() {
        // `transition(...)` and `on_press(...)` are paren-delimited, so a box can be animated AND clickable
        // on one line in any order, and a closure with nested parens is captured whole.
        let src = "[logic]\nlet count = signal(0i32);\n[view]\nbox fill:primary transition(fill 200ms ease-out) on_press(|| $count.update(|n| *n += 1))\n    text \"x\"\n";
        let out = crate::transpile_source_with_theme(src, "demo", None, None).unwrap();
        let code = &out.rust_code;
        assert!(
            code.contains(".on_press("),
            "paren on_press should emit a handler:\n{code}"
        );
        assert!(
            code.contains("count.update(|n| *n += 1)"),
            "nested-paren closure should be captured whole:\n{code}"
        );
        assert!(
            code.contains("motion::Animated::new"),
            "transition should still wire even when it precedes on_press:\n{code}"
        );
    }

    // A `box` with a `hover(...)` override wires `.on_hover_style(...)`.
    #[test]
    fn box_hover_emits_on_hover_style() {
        let src = "[view]\nbox fill:#101010 hover(fill:#f0f0f0 stroke:#ff0000) radius:10\n    text \"x\"\n";
        let out = crate::transpile_source_with_theme(src, "demo", None, None).unwrap();
        assert!(
            out.rust_code.contains(".on_hover_style("),
            "hover(...) should wire on_hover_style:\n{}",
            out.rust_code
        );
    }

    // A plain `col` gains a hover style, so it must upgrade to a StyledContainer (which has a background).
    #[test]
    fn plain_col_with_hover_upgrades_to_styled_container() {
        let src = "[view]\ncol hover(fill:#f0f0f0)\n    text \"x\"\n";
        let out = crate::transpile_source_with_theme(src, "demo", None, None).unwrap();
        let code = &out.rust_code;
        assert!(
            code.contains("StyledContainer::new(ctx,"),
            "a col with hover should become a StyledContainer:\n{code}"
        );
        assert!(
            code.contains(".on_hover_style("),
            "and wire on_hover_style:\n{code}"
        );
    }

    // A component whose view uses a `children` placeholder takes a `Slots` arg and drains the default slot.
    #[test]
    fn component_default_slot_takes_slots_arg() {
        let src = "[view]\nbox fill:#101010 pad:16\n    children\n";
        let out = crate::transpile_source_with_theme(src, "card", None, None).unwrap();
        let code = &out.rust_code;
        assert!(
            code.contains("mut __slots: Slots"),
            "a slotted component takes a Slots argument:\n{code}"
        );
        assert!(
            code.contains("__children.extend(__slots.take_default());"),
            "the default slot splices take_default():\n{code}"
        );
    }

    // Named + default slots drain their respective buckets.
    #[test]
    fn component_named_and_default_slots() {
        let src = "[view]\nbox pad:16\n    children name:\"header\"\n    children\n";
        let out = crate::transpile_source_with_theme(src, "panel", None, None).unwrap();
        let code = &out.rust_code;
        assert!(
            code.contains("__children.extend(__slots.take(\"header\"));"),
            "named slot drains take(\"header\"):\n{code}"
        );
        assert!(
            code.contains("__children.extend(__slots.take_default());"),
            "default slot drains take_default():\n{code}"
        );
    }

    // Calling a component with markup children builds a Slots value and passes it as the trailing arg.
    #[test]
    fn component_call_with_children_builds_slots() {
        let src = "[view]\ncard\n    text \"hi\"\n";
        let out = crate::transpile_source_with_theme(src, "demo", None, None).unwrap();
        let code = &out.rust_code;
        assert!(
            code.contains("let mut __slots = Slots::new();"),
            "a call with children builds a Slots:\n{code}"
        );
        assert!(
            code.contains("card(ctx, __slots)?"),
            "the Slots is the trailing arg:\n{code}"
        );
    }

    // A child written with `slot:"name"` is routed to that named slot; the `slot` attr is not a prop.
    #[test]
    fn component_call_routes_named_slot() {
        let src = "[view]\npanel\n    text \"T\" slot:\"header\"\n    text \"B\"\n";
        let out = crate::transpile_source_with_theme(src, "demo", None, None).unwrap();
        let code = &out.rust_code;
        assert!(
            code.contains("__slots.push(Some(\"header\"), box_item("),
            "slot:\"header\" routes to the named slot:\n{code}"
        );
        assert!(
            code.contains("__children.push(box_item("),
            "a bare child goes to the default slot:\n{code}"
        );
    }

    // A bare flag prop becomes a bool `true`.
    #[test]
    fn component_bool_flag_prop() {
        let src = "[view]\ncard elevated\n    text \"x\"\n";
        let out = crate::transpile_source_with_theme(src, "demo", None, None).unwrap();
        let code = &out.rust_code;
        assert!(
            code.contains("crate::CardProps { elevated: true }"),
            "a bare flag prop should be bool true:\n{code}"
        );
    }

    // A lone `$signal` prop passes the cloned handle.
    #[test]
    fn component_signal_prop_clones_handle() {
        let src = "[logic]\nlet count = signal(0i32);\n[view]\ncard count:$count\n    text \"x\"\n";
        let out = crate::transpile_source_with_theme(src, "demo", None, None).unwrap();
        let code = &out.rust_code;
        assert!(
            code.contains("count: count.clone()"),
            "a $signal prop should pass the cloned handle:\n{code}"
        );
    }

    // A closure prop is boxed, with its captured signal cloned and `$` sugar desugared.
    #[test]
    fn component_closure_prop_is_boxed() {
        let src = "[logic]\nlet count = signal(0i32);\n[view]\ncard on_tap(|| $count += 1)\n    text \"x\"\n";
        let out = crate::transpile_source_with_theme(src, "demo", None, None).unwrap();
        let code = &out.rust_code;
        assert!(
            code.contains("Box::new("),
            "a closure prop should be boxed:\n{code}"
        );
        assert!(
            code.contains("count.update(|__v| *__v += 1)"),
            "the closure's $ sugar should desugar:\n{code}"
        );
    }

    fn sig(props_default: bool, fields: &[&str], has_slot: bool) -> crate::ComponentSig {
        crate::ComponentSig {
            has_props: !fields.is_empty(),
            props_default,
            prop_fields: fields.iter().map(|s| s.to_string()).collect(),
            has_slot,
        }
    }

    // With the registry, a childless call to a slotted component still passes a `Slots` arg (empty), so it
    // matches the callee's 3-arg signature instead of erroring "expected 3 arguments, found 2".
    #[test]
    fn childless_slotted_call_passes_empty_slots() {
        let mut reg = crate::ComponentRegistry::new();
        reg.insert("card".to_string(), sig(true, &["gap"], true));
        let out =
            crate::transpile_source_full("[view]\ncard\n", "demo", None, None, Some(&reg)).unwrap();
        assert!(
            out.rust_code
                .contains("card(ctx, crate::CardProps { ..Default::default() }, Slots::new())?"),
            "childless slotted call should pass defaulted props + empty Slots:\n{}",
            out.rust_code
        );
    }

    // A call that omits some fields of a `Default`-deriving component adds `..Default::default()`.
    #[test]
    fn omitted_prop_adds_default_update() {
        let mut reg = crate::ComponentRegistry::new();
        reg.insert(
            "doc_header".to_string(),
            sig(true, &["kicker", "title", "desc"], false),
        );
        let out = crate::transpile_source_full(
            "[view]\ndoc_header title:\"X\"\n",
            "demo",
            None,
            None,
            Some(&reg),
        )
        .unwrap();
        assert!(
            out.rust_code
                .contains("crate::DocHeaderProps { title: \"X\", ..Default::default() }"),
            "an omitted field should default:\n{}",
            out.rust_code
        );
    }

    // A full-field call omits `..Default::default()` (so a clean, `Default`-agnostic struct literal, no
    // clippy::needless_update), even when the component derives Default.
    #[test]
    fn full_field_call_omits_default_update() {
        let mut reg = crate::ComponentRegistry::new();
        reg.insert(
            "prop_row".to_string(),
            sig(true, &["name", "values", "about"], false),
        );
        let out = crate::transpile_source_full(
            "[view]\nprop_row name:\"a\" values:\"b\" about:\"c\"\n",
            "demo",
            None,
            None,
            Some(&reg),
        )
        .unwrap();
        assert!(
            !out.rust_code.contains("..Default::default()"),
            "a full-field call must not add ..Default::default():\n{}",
            out.rust_code
        );
    }

    // Rich text: weight keyword, italic flag, and alignment become TextStyle builder calls.
    #[test]
    fn text_rich_weight_italic_align() {
        let src = "[view]\ntext \"Hi\" weight:bold italic align:center\n";
        let out = crate::transpile_source_with_theme(src, "demo", None, None).unwrap();
        let code = &out.rust_code;
        assert!(
            code.contains(".with_weight(700)"),
            "weight keyword:\n{code}"
        );
        assert!(code.contains(".with_italic(true)"), "italic flag:\n{code}");
        assert!(
            code.contains(".with_align(TextAlign::Center)"),
            "align:\n{code}"
        );
    }

    // Numeric weight and `align:right` map correctly; absent italic emits no builder call.
    #[test]
    fn text_rich_numeric_weight_and_align_end() {
        let src = "[view]\ntext \"Hi\" weight:600 align:right\n";
        let code = crate::transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains(".with_weight(600)"),
            "numeric weight:\n{code}"
        );
        assert!(
            code.contains(".with_align(TextAlign::End)"),
            "align end:\n{code}"
        );
        assert!(
            !code.contains(".with_italic"),
            "no italic when absent:\n{code}"
        );
    }

    // `lines:N` and the `ellipsis` flag become max-lines / ellipsis builder calls.
    #[test]
    fn text_lines_and_ellipsis() {
        let src = "[view]\ntext \"Long copy here\" lines:2 ellipsis max_width:200\n";
        let code = crate::transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(code.contains(".with_max_lines(2)"), "max_lines:\n{code}");
        assert!(code.contains(".with_ellipsis(true)"), "ellipsis:\n{code}");
    }

    // A `$`-source `for` with a `key` clause emits a ReactiveList (source read, key closure, item builder).
    #[test]
    fn reactive_for_emits_reactive_list() {
        let src = "[logic]\nlet items = signal(vec![1i32, 2, 3]);\n[view]\ncol\n    for n in $items key *n\n        text \"x\"\n";
        let code = crate::transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains("ReactiveList::new("),
            "reactive for should build a ReactiveList:\n{code}"
        );
        assert!(
            code.contains("move || items.get()"),
            "source reads the signal:\n{code}"
        );
        assert!(
            code.contains("|n| *n"),
            "key closure from `key *n`:\n{code}"
        );
        assert!(
            code.contains("move |ctx: &mut WidgetCtx, n|"),
            "item builder closure:\n{code}"
        );
    }

    // A reactive `for` without a `key` clause is a compile_error (reconciliation needs identity).
    #[test]
    fn reactive_for_without_key_errors() {
        let src = "[view]\ncol\n    for n in $items\n        text \"x\"\n";
        let code = crate::transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains("compile_error!") && code.contains("key"),
            "a keyless reactive for should emit a compile_error:\n{code}"
        );
    }

    // A non-`$` `for` stays the one-time construction loop.
    #[test]
    fn static_for_stays_construction_loop() {
        let src = "[view]\ncol\n    for n in 0..3\n        text \"x\"\n";
        let code = crate::transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            !code.contains("ReactiveList"),
            "a plain for must not be reactive:\n{code}"
        );
        assert!(
            code.contains("for n in 0..3"),
            "construction loop preserved:\n{code}"
        );
    }

    // An `if` whose condition reads a signal (`$`) becomes a reactive conditional: a single-item
    // ReactiveList keyed on the bool, whose builder holds the then/else branches.
    #[test]
    fn reactive_if_emits_reactive_list() {
        let src = "[logic]\nlet show = signal(true);\n[view]\ncol\n    if $show\n        text \"yes\"\n    else\n        text \"no\"\n";
        let code = crate::transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains("ReactiveList::new("),
            "reactive if builds a ReactiveList:\n{code}"
        );
        assert!(
            code.contains("move || vec![show.get()]"),
            "condition read as source:\n{code}"
        );
        assert!(
            code.contains("|__cond: &bool| *__cond"),
            "bool key:\n{code}"
        );
        assert!(code.contains("if __cond"), "branch selector:\n{code}");
    }

    // A plain (non-`$`) condition stays a one-shot construction `if`.
    #[test]
    fn static_if_stays_construction() {
        let src = "[view]\ncol\n    if some_flag\n        text \"x\"\n";
        let code = crate::transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            !code.contains("ReactiveList"),
            "a plain if must not be reactive:\n{code}"
        );
        assert!(
            code.contains("some_flag"),
            "condition emitted verbatim:\n{code}"
        );
    }

    // Without a registry, behavior is unchanged: a childless unknown component is a bare `tag(ctx)?` call
    // (no slot arg, no default), preserving the per-file fallback.
    #[test]
    fn no_registry_keeps_flat_call() {
        let out =
            crate::transpile_source_with_theme("[view]\nmy_card\n", "demo", None, None).unwrap();
        assert!(
            out.rust_code.contains("my_card(ctx)?"),
            "without a registry a no-attr call stays flat:\n{}",
            out.rust_code
        );
    }
}
