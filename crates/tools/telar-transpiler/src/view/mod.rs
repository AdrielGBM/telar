//! Generates the body of the component function from the `[view]` section.

mod canvas;
mod component;
mod container;
mod control_flow;
mod input;
mod interp;
mod lazy;
mod media;
mod path;
mod scroll;
mod signals;
mod style_helpers;
mod text;

use std::collections::HashMap;
use std::fmt::Write;
use std::path::{Path, PathBuf};

use telar_parser::{Element, IfBlock, StyleClass, StyleConstant, ViewNode};

use crate::naming::contains_ident;

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
        ChildEmit::Fragment { name, code } => ChildEmit::Fragment {
            name,
            code: wrap(code),
        },
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

/// How a container collects its children: a `children![...]` literal (all static, no control flow), a
/// `Vec<Box<dyn LayoutItem>>` mutated by static control flow, or a `Vec<ChildSlot>` when a reactive
/// fragment is among the siblings (so it and the statics reconcile into the same node — the transparent
/// `for`/`if`).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChildMode {
    Literal,
    Vec,
    Slots,
}

/// The child accumulator currently in scope: the `Vec` variable that `if`/`for` bodies push into, and
/// whether it holds `ChildSlot`s (slot mode) vs `Box<dyn LayoutItem>` (vec mode).
pub(crate) struct ChildSink {
    var: &'static str,
    slots: bool,
}

/// Whether a node forces its parent into slot mode: a *reactive* `for`/`if` (a `$`-driven source) becomes
/// a transparent fragment, which only the `ChildSlot`/`from_slots` path can host. A static `for`/`if` does
/// not — it pushes its widgets directly, so a `Vec` accumulator suffices.
pub(crate) fn forces_fragment(node: &ViewNode) -> bool {
    match node {
        ViewNode::ForBlock(block) => block.iterable.trim_start().starts_with('$'),
        ViewNode::IfBlock(block) => block.condition.contains('$'),
        _ => false,
    }
}

/// A piece of generated child code together with how it contributes to a parent's child collection.
pub(crate) enum ChildEmit {
    /// A simple widget bound to `name`, pushable directly.
    Simple { name: String, code: String },
    /// Control flow (`if`/`for`) that mutates a child vector in place.
    Dynamic { code: String },
    /// A reactive `for`/`if` region bound to `name` as a `ChildSlot::Dynamic` (a transparent fragment that
    /// reconciles into the host container's node). Forces the parent to collect `ChildSlot`s and build via
    /// `from_slots`, so the region's items are real siblings of the static children.
    Fragment { name: String, code: String },
}

/// Collapses a set of already-bound widget `names` into a single content expression: an empty column
/// for none, the lone widget for one, or a `Container::column` wrapping all of them for several. Shared
/// by `generate_root` (which boxes the result as the view's return value) and `emit_scroll`'s static
/// branch (`LayoutScrollArea` takes a single content item).
fn wrap_as_single_content(names: &[String]) -> String {
    match names {
        [] => "Container::column(children![])?".to_string(),
        [only] => only.clone(),
        _ => format!("Container::column(children![{}])?", names.join(", ")),
    }
}

pub struct ViewGen<'a> {
    /// Declared style classes, used to validate class references in elements.
    classes: &'a [StyleClass],
    constants: &'a [StyleConstant],
    /// Per-widget-type variable counters, keyed by the descriptive prefix.
    counters: HashMap<String, usize>,
    /// When set, `[style]` color references resolve to `use_theme::<Type>().field` instead of generated `COLOR_*` consts, so theme switching takes effect.
    theme_type: Option<String>,
    /// Identifiers the `[logic]` zone binds. A bare name in the view resolves to one of these before the theme
    /// is consulted, so a local shadows a same-named token rather than the other way round — see
    /// [`crate::signal_scan::scan_locals`].
    locals: Vec<String>,
    /// Names the `[logic]` zone bound to a `signal(…)` or `memo(…)`. Read to tell a genuinely static iterable
    /// from one that reads reactive state without the `$` that would make the loop follow it — see
    /// [`ViewGen::signal_named_in`].
    signals: Vec<String>,
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
    /// Stack of child accumulators (see [`ChildSink`]); the top is the one an emitted `if`/`for` body
    /// pushes into. Pushed before a container's children are emitted, popped after.
    child_sinks: Vec<ChildSink>,
    /// How many reactive branch/item closures enclose the node being emitted. Non-zero means the code being
    /// generated may run more than once for the same content, which is what makes a one-shot `widget`
    /// reference unsound there — see [`ViewGen::in_reactive_region`].
    reactive_depth: usize,
    /// Whether each enclosing container lays its children out horizontally. A boxed reactive region reads
    /// the top to build its own node the same way round — an `if`/`for` inside a `row` runs horizontally.
    host_rows: Vec<bool>,
    /// The in-scope binding holding the enclosing scroll's live viewport, when the node being emitted sits
    /// inside one that exposed it. `VirtualList` needs it to know which rows are on screen, and only a `scroll`
    /// can supply it — so a `virtual` loop outside one is an error rather than a surprise at runtime.
    scroll_viewport: Option<String>,
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
            locals: Vec::new(),
            signals: Vec::new(),
            indent: 1,
            loop_variables: Vec::new(),
            transition_count: 0,
            base_dir: base_dir.map(Path::to_path_buf),
            baked_asset_count: 0,
            registry: None,
            child_sinks: Vec::new(),
            host_rows: Vec::new(),
            reactive_depth: 0,
            scroll_viewport: None,
        }
    }

    /// Whether the node being emitted sits inside a reactive branch or item closure, i.e. inside code that may
    /// build the same content again after the region has disposed it.
    pub(super) fn in_reactive_region(&self) -> bool {
        self.reactive_depth > 0
    }

    /// Runs `emit` with the reactive region marked, so anything emitted inside knows it may be rebuilt.
    pub(super) fn in_reactive<R>(&mut self, emit: impl FnOnce(&mut Self) -> R) -> R {
        self.reactive_depth += 1;
        let result = emit(self);
        self.reactive_depth -= 1;
        result
    }

    /// Whether the container the node being emitted sits directly inside lays its children horizontally.
    pub(super) fn host_is_row(&self) -> bool {
        self.host_rows.last().copied().unwrap_or(false)
    }

    /// Runs `emit` with `is_row` recorded as the enclosing container's axis.
    pub(super) fn within_host<R>(&mut self, is_row: bool, emit: impl FnOnce(&mut Self) -> R) -> R {
        self.host_rows.push(is_row);
        let result = emit(self);
        self.host_rows.pop();
        result
    }

    /// Runs `emit` with a child-accumulator context in scope, so `if`/`for` bodies emitted inside push into
    /// the right `Vec` in the right shape. No sink is pushed for [`ChildMode::Literal`] (nothing pushes).
    fn with_child_sink<R>(&mut self, mode: ChildMode, emit: impl FnOnce(&mut Self) -> R) -> R {
        let pushed = mode != ChildMode::Literal;
        if pushed {
            self.child_sinks.push(ChildSink {
                var: if mode == ChildMode::Slots {
                    "__slots"
                } else {
                    "__children"
                },
                slots: mode == ChildMode::Slots,
            });
        }
        let result = emit(self);
        if pushed {
            self.child_sinks.pop();
        }
        result
    }

    /// Emits a push of static widget `name` into the current child accumulator, in its shape (a bare
    /// `box_item` for a vec sink, wrapped in `ChildSlot::stat` for a slot sink).
    fn push_static_child(&self, code: &mut String, pad: &str, name: &str) {
        match self.child_sinks.last() {
            Some(sink) if sink.slots => {
                let _ = writeln!(
                    code,
                    "{pad}{}.push(ChildSlot::stat(box_item({name})));",
                    sink.var
                );
            }
            Some(sink) => {
                let _ = writeln!(code, "{pad}{}.push(box_item({name}));", sink.var);
            }
            None => {}
        }
    }

    /// Emits a push of a reactive fragment `name` (a `ChildSlot::Dynamic`) into the current slot sink.
    fn push_fragment_child(&self, code: &mut String, pad: &str, name: &str) {
        if let Some(sink) = self.child_sinks.last() {
            let _ = writeln!(code, "{pad}{}.push({name});", sink.var);
        }
    }

    /// Whether the child accumulator in scope hosts `ChildSlot`s — i.e. a reactive `for`/`if` here can be a
    /// transparent fragment. Outside a slot host (component-slot children, a bare root, overlay/scroll) it
    /// must fall back to a boxed `ReactiveList`.
    fn in_slot_host(&self) -> bool {
        self.child_sinks.last().is_some_and(|sink| sink.slots)
    }

    /// The child-collection mode a container/branch with these AST children needs.
    fn child_mode(children: &[ViewNode]) -> ChildMode {
        if children.iter().any(forces_fragment) {
            ChildMode::Slots
        } else if children.iter().any(forces_child_vec) {
            ChildMode::Vec
        } else {
            ChildMode::Literal
        }
    }

    /// Attaches the names the `[logic]` zone binds, so a bare identifier in the view reaches them before the
    /// theme. A preview has no logic zone and so passes none.
    pub(crate) fn with_locals(mut self, locals: Vec<String>) -> Self {
        self.locals = locals;
        self
    }

    pub(crate) fn with_signals(mut self, signals: Vec<String>) -> Self {
        self.signals = signals;
        self
    }

    /// Whether `name` is a binding the logic zone made, which a bare reference in the view means before any
    /// same-named theme token.
    pub(super) fn is_local(&self, name: &str) -> bool {
        self.locals.iter().any(|local| local == name)
    }

    /// The first signal `code` mentions by name, skipping strings and comments.
    ///
    /// Names, not types: `[logic]` is spliced through verbatim and never type-checked here, so the only thing
    /// this can honestly say is *this text refers to something the author declared reactive*. Enough for the
    /// one question it is asked — whether an expression that looks static is reading state that moves.
    pub(super) fn signal_named_in(&self, code: &str) -> Option<&str> {
        self.signals
            .iter()
            .find(|name| contains_ident(code, name))
            .map(String::as_str)
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
            "col" | "column" => "col",
            "row" => "row",
            "box" => "sbox",
            "overlay" => "overlay",
            "lazy" => "lazy",
            "img" | "image" => "img",
            "input" => "input",
            "svg" => "svg",
            "path" => "path",
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
        // A single static `if`/`if-else` root returns its branch directly, so the branch element becomes the
        // component root with no injected column that would trap a `row align:stretch` at content height. A
        // reactive (`$`) condition keeps the fragment-swap path below.
        if let [ViewNode::IfBlock(block)] = nodes
            && !block.condition.contains('$')
        {
            return self.generate_root_if(block);
        }

        let mut out = String::new();
        let mode = Self::child_mode(nodes);

        // A reactive fragment (or static control flow) at the root has no explicit container to attach to,
        // so wrap the roots in one flex-column container built the matching way (`from_slots` for a
        // fragment, `column` otherwise).
        if mode != ChildMode::Literal {
            let child_emits: Vec<ChildEmit> =
                self.with_child_sink(mode, |g| nodes.iter().map(|n| g.emit_node(n)).collect());
            let pad = self.indent_str();
            let expr = self.emit_children_collection(&mut out, &child_emits, &pad, mode, &[]);
            let content = if mode == ChildMode::Slots {
                format!("Container::from_slots(LayoutStyle::new().flex_column(), {expr})?")
            } else {
                format!("Container::column({expr})?")
            };
            let _ = write!(out, "{pad}Ok(Box::new({content}))");
            return out;
        }

        let mut roots = Vec::new();
        for node in nodes {
            match self.emit_node(node) {
                ChildEmit::Simple { name, code } => {
                    out.push_str(&code);
                    out.push('\n');
                    roots.push(name);
                }
                ChildEmit::Dynamic { code } | ChildEmit::Fragment { code, .. } => {
                    out.push_str(&code);
                    out.push('\n');
                }
            }
        }

        let pad = self.indent_str();
        let content = wrap_as_single_content(&roots);
        let _ = write!(out, "{pad}Ok(Box::new({content}))");

        out
    }

    /// Generates the body for a view that is a single static `if`/`if-else`: each branch returns its content
    /// directly (via [`Self::emit_content_cell`]), so the chosen branch is the component root with no wrapping
    /// column. A missing `else` returns an empty column. Source markers/spans are preserved for cursor mapping.
    fn generate_root_if(&mut self, block: &IfBlock) -> String {
        let mut out = String::new();
        let pad = self.indent_str();
        let cond = block.condition.trim();
        let marker = expr_marker(block.condition_start, cond.len());
        let src0 = block.line.saturating_sub(1);
        let _ = writeln!(out, "{SRC_PUSH}{src0}");
        let _ = writeln!(out, "{pad}if {marker}{cond} {{");
        self.indent += 1;
        let then_cell = self.emit_content_cell(&block.then_branch, &mut out);
        let ipad = self.indent_str();
        let _ = writeln!(out, "{ipad}Ok(Box::new({then_cell}))");
        self.indent -= 1;
        let _ = writeln!(out, "{pad}}} else {{");
        self.indent += 1;
        let ipad = self.indent_str();
        match &block.else_branch {
            Some(else_branch) => {
                let else_cell = self.emit_content_cell(else_branch, &mut out);
                let _ = writeln!(out, "{ipad}Ok(Box::new({else_cell}))");
            }
            None => {
                let _ = writeln!(out, "{ipad}Ok(Box::new(Container::column(children![])?))");
            }
        }
        self.indent -= 1;
        let _ = writeln!(out, "{pad}}}");
        let _ = write!(out, "{SRC_POP}");
        out
    }

    fn emit_node(&mut self, node: &ViewNode) -> ChildEmit {
        let emit = match node {
            ViewNode::Element(el) => self.emit_element(el),
            ViewNode::LetStmt(stmt) => ChildEmit::Dynamic {
                code: format!(
                    "{}{}{};",
                    self.indent_str(),
                    expr_marker(stmt.source_start, stmt.source.len()),
                    stmt.source
                ),
            },
            ViewNode::IfBlock(block) => self.emit_if(block),
            ViewNode::ForBlock(block) => self.emit_for(block),
            ViewNode::MatchBlock(block) => self.emit_match(block),
            // Carried through as a Rust comment: the generated file is what a diagnostic points at, and a note
            // explaining the markup is worth as much there as it is in the `.rsx`.
            ViewNode::Comment(text) => ChildEmit::Dynamic {
                code: format!("{}{text}", self.indent_str()),
            },
        };
        // Bracket this node's generated lines with source markers so the transpiler can map them back to the `.rsx` line. Nested nodes nest their own markers; `let` statements have no line of their own and inherit the enclosing node's mapping.
        match node {
            ViewNode::Element(el) => wrap_source_markers(emit, el.line),
            ViewNode::IfBlock(block) => wrap_source_markers(emit, block.line),
            ViewNode::ForBlock(block) => wrap_source_markers(emit, block.line),
            ViewNode::MatchBlock(block) => wrap_source_markers(emit, block.line),
            ViewNode::LetStmt(_) | ViewNode::Comment(_) => emit,
        }
    }

    /// Names any attribute a built-in tag does not accept, instead of mapping it to a builder call that does
    /// nothing. It checks the same table the analyzer completes from, so an attribute the editor never suggests
    /// is now one the build refuses too — before this, `cols:` on a plain `box` compiled and did nothing.
    /// Component tags are exempt: their keys are `Props` fields, which rustc already checks.
    fn unknown_attr_errors(&self, el: &Element) -> String {
        let allowed = crate::registry::tag_attr_keys(&el.tag);
        if allowed.is_empty() {
            return String::new();
        }
        let pad = self.indent_str();
        el.attributes
            .iter()
            .filter(|attr| !allowed.contains(&attr.key.as_str()))
            .map(|attr| {
                format!(
                    "{pad}compile_error!({});\n",
                    signals::rust_str(&format!(
                        "`{}` is not an attribute of `{}`",
                        attr.key, el.tag
                    ))
                )
            })
            .collect()
    }

    fn emit_element(&mut self, el: &Element) -> ChildEmit {
        let unknown = self.unknown_attr_errors(el);
        let emit = self.emit_element_inner(el);
        if unknown.is_empty() {
            return emit;
        }
        match emit {
            ChildEmit::Simple { name, code } => ChildEmit::Simple {
                name,
                code: format!("{unknown}{code}"),
            },
            ChildEmit::Fragment { name, code } => ChildEmit::Fragment {
                name,
                code: format!("{unknown}{code}"),
            },
            ChildEmit::Dynamic { code } => ChildEmit::Dynamic {
                code: format!("{unknown}{code}"),
            },
        }
    }

    fn emit_element_inner(&mut self, el: &Element) -> ChildEmit {
        match el.tag.as_str() {
            "text" => self.emit_text(el),
            "col" | "row" | "grid" => self.emit_container(el),
            "box" => self.emit_box(el),
            "overlay" => self.emit_overlay(el),
            "lazy" => self.emit_lazy(el),
            "img" | "image" => self.emit_image(el),
            "input" => self.emit_input(el),
            "svg" => self.emit_svg(el),
            "path" => self.emit_path(el),
            "scroll" => self.emit_scroll(el),
            "canvas" => self.emit_canvas(el),
            "widget" => self.emit_widget_ref(el),
            "build" => self.emit_build_expr(el),
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
        ViewNode::IfBlock(_)
        | ViewNode::ForBlock(_)
        | ViewNode::MatchBlock(_)
        | ViewNode::LetStmt(_) => true,
        ViewNode::Element(el) => el.tag == "children",
        // A note builds nothing, so it must not push a sibling list into the vec shape.
        ViewNode::Comment(_) => false,
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
        let src = "[logic]\nlet canvas = build_canvas()?;\n[view]\nwidget \"canvas\"\n";
        let out = crate::transpile_source_with_theme(src, "my_section", None, None).unwrap();
        assert!(out.rust_code.contains("Ok(Box::new(canvas))"));
    }

    #[test]
    fn build_splices_an_expression_evaluated_at_each_construction_point() {
        // The point of `build`: inside a reactive region it re-runs, so nothing is moved twice.
        let src = "[logic]\nlet items = memo(move || vec![1, 2]);\n[view]\nrow\n    for id in $items key *id\n        build \"icon_view(id)?\"\n";
        let out = crate::transpile_source_with_theme(src, "demo", None, None).unwrap();
        let code = &out.rust_code;
        assert!(
            code.contains("Ok(box_item(icon_view(id)?))"),
            "the expression is emitted inside the item closure:\n{code}"
        );
        assert!(
            !code.contains("compile_error!"),
            "and it is not an error:\n{code}"
        );
    }

    #[test]
    fn build_works_alongside_widget_in_a_static_view() {
        let src = "[logic]\nlet icon = make_icon()?;\n[view]\nrow\n    widget \"icon\"\n    build \"other()?\"\n";
        let out = crate::transpile_source_with_theme(src, "demo", None, None).unwrap();
        assert!(
            out.rust_code.contains("children![icon, other()?]"),
            "a spliced binding and a built expression are siblings:\n{}",
            out.rust_code
        );
    }

    #[test]
    fn widget_inside_a_reactive_region_names_the_rule_and_the_fix() {
        // Without this the author gets rustc's E0507 pointing into generated code they never wrote.
        for src in [
            "[logic]\nlet icon = make_icon()?;\nlet shown = memo(move || true);\n[view]\nrow\n    if $shown\n        widget \"icon\"\n",
            "[logic]\nlet icon = make_icon()?;\nlet items = memo(move || vec![1]);\n[view]\nrow\n    for id in $items key *id\n        widget \"icon\"\n",
            // Nested one level down: a plain container inside a reactive branch is rebuilt with it.
            "[logic]\nlet icon = make_icon()?;\nlet shown = memo(move || true);\n[view]\nrow\n    if $shown\n        col\n            widget \"icon\"\n",
        ] {
            let out = crate::transpile_source_with_theme(src, "demo", None, None).unwrap();
            let code = &out.rust_code;
            assert!(
                code.contains("compile_error!")
                    && code.contains("cannot be used inside a reactive"),
                "a reactive `widget` must explain itself:\n{code}"
            );
            assert!(
                code.contains("build"),
                "and point at the alternative:\n{code}"
            );
        }
    }

    #[test]
    fn a_non_reactive_branch_still_takes_a_widget() {
        // A construction-time `if` picks its branch once, so the guard is about rebuilding, not about branching.
        let src = "[logic]\nlet icon = make_icon()?;\nlet vertical = true;\n[view]\nrow\n    if vertical\n        widget \"icon\"\n";
        let out = crate::transpile_source_with_theme(src, "demo", None, None).unwrap();
        assert!(
            !out.rust_code.contains("compile_error!"),
            "a one-shot `if` is not a reactive region:\n{}",
            out.rust_code
        );
    }

    #[test]
    fn build_rejects_an_empty_or_truncated_expression() {
        for (src, why) in [
            ("[logic]\n[view]\nbuild \"\"\n", "empty"),
            ("[logic]\n[view]\nbuild \"icon_view(name\"\n", "unbalanced"),
        ] {
            let out = crate::transpile_source_with_theme(src, "demo", None, None).unwrap();
            assert!(
                out.rust_code.contains("compile_error!"),
                "a {why} build expression should not reach rustc as broken syntax:\n{}",
                out.rust_code
            );
        }
        // A bracket inside a string literal is not an unbalanced bracket.
        let ok = crate::transpile_source_with_theme(
            "[logic]\n[view]\nbuild \"label(\\\")\\\")?\"\n",
            "demo",
            None,
            None,
        )
        .unwrap();
        assert!(!ok.rust_code.contains("compile_error!"), "{}", ok.rust_code);
    }

    #[test]
    fn widget_ref_invalid_identifier_errors() {
        // A non-identifier `widget` reference emits a `compile_error!` instead of splicing broken code.
        let src = "[logic]\n[view]\nwidget \"not an ident\"\n";
        let out = crate::transpile_source_with_theme(src, "demo", None, None).unwrap();
        assert!(
            out.rust_code.contains("compile_error!"),
            "invalid widget ref should emit compile_error!:\n{}",
            out.rust_code
        );
    }

    #[test]
    fn inline_field_default_synthesizes_default_impl() {
        // `field: Type = expr` sugar: the emitted struct drops the default (invalid Rust otherwise) and a
        // synthesized `Default` impl carries it; the `#[derive(Default)]` is stripped to avoid a collision.
        let src = "[logic]\n#[derive(Default)]\npub struct Props {\n    pub gap: f32,\n    pub pad: f32 = 16.0,\n}\n[view]\ntext \"x\"\n";
        let out = crate::transpile_source_with_theme(src, "card", None, None).unwrap();
        let code = &out.rust_code;
        assert!(
            code.contains("pub pad: f32,"),
            "field default not stripped:\n{code}"
        );
        assert!(
            !code.contains("= 16.0"),
            "inline default leaked into struct:\n{code}"
        );
        assert!(
            code.contains("impl Default for CardProps"),
            "missing synthesized Default impl:\n{code}"
        );
        assert!(
            code.contains("pad: 16.0"),
            "default value missing from impl:\n{code}"
        );
        assert!(
            code.contains("gap: Default::default()"),
            "non-defaulted field missing from impl:\n{code}"
        );
        assert!(
            !code.contains("#[derive(Default)]"),
            "derive(Default) should be stripped when an impl is synthesized:\n{code}"
        );
    }

    #[test]
    fn inline_default_makes_component_default_constructible() {
        // A struct with an inline default is default-constructible even without `#[derive(Default)]`, so
        // the registry lets callers omit its props (they fall through to `..Default::default()`).
        let sig = crate::scan_component_sig(
            "[logic]\npub struct Props {\n    pub pad: f32 = 16.0,\n}\n[view]\ntext \"x\"\n",
        );
        assert!(
            sig.props_default,
            "inline default should mark props as default-constructible"
        );
        assert!(sig.prop_fields.contains(&"pad".to_string()));
    }

    #[test]
    fn canvas_with_rect_and_text_children() {
        let src = "[logic]\n[view]\ncanvas width:100 height:50\n    rect fill:#3c77fa radius:8\n    text \"hi\" x:0 y:4 w:full h:42 size:12 color:white\n";
        let out = crate::transpile_source_with_theme(src, "demo", None, None).unwrap();
        let code = &out.rust_code;
        assert!(code.contains("Canvas::new("), "missing Canvas::new");
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
            out.rust_code.contains("my_card()?"),
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
            code.contains("StyledContainer::new(style_card()"),
            "painted col should be a StyledContainer:\n{code}"
        );
        // The class's fill reaches the RectStyle.
        assert!(
            code.contains("with_fill"),
            "class fill should reach the RectStyle:\n{code}"
        );
    }

    #[test]
    fn multiple_classes_compose_layout_and_paint() {
        // `box @a @b`: the first class is the base (its `style_*()` fn), the second's layout props are chained
        // on top (so it overrides), and a later class's paint still reaches the RectStyle.
        let src = "[style]\n@a\n    align: center\n@b\n    align: start\n    fill: #ff0000\n[view]\nbox @a @b\n    text \"hi\"\n";
        let out = crate::transpile_source_with_theme(src, "demo", None, None).unwrap();
        let code = &out.rust_code;
        // First class as base fn, second class's align chained directly after it (later wins at runtime).
        assert!(
            code.contains("style_a().align_items(AlignItems::START)"),
            "second class's layout should compose on top of the first:\n{code}"
        );
        // The later class's fill reaches the RectStyle (paint composes too).
        assert!(
            code.contains("with_fill"),
            "a class's fill should reach the RectStyle:\n{code}"
        );
    }

    #[test]
    fn classed_box_is_a_flex_container() {
        // A `style_*()` class fn is `LayoutStyle::new()` = display:block, where align/justify are no-ops.
        // A classed `box` must still get `.flex_column()` (like a plain box) so its children actually centre.
        let src = "[style]\n@center\n    align: center\n    justify: center\n[view]\nbox @center width:100 height:60\n    text \"hi\"\n";
        let out = crate::transpile_source_with_theme(src, "demo", None, None).unwrap();
        assert!(
            out.rust_code.contains("style_center().flex_column()"),
            "a classed box must be a flex container so align/justify apply:\n{}",
            out.rust_code
        );
    }

    #[test]
    fn container_on_press_emits_click_handler() {
        // A painted `box` (StyledContainer) and a plain `col` (Container) both wire `.on_press`.
        let src = "[logic]\nlet n = signal(0i32);\n[view]\ncol\n    box fill:primary on_press(|| $n.update(|v| *v += 1))\n    col on_press(|| $n.set(0))\n        text \"x\"\n";
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
        let src = "[logic]\nlet count = signal(0i32);\n[view]\ncol\n    button on_press(|| $count += 1)\n    button on_press(|| $count -= 2)\n";
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
        let src = "[logic]\nlet flag = signal(false);\nlet count = signal(0i32);\n[view]\ncol\n    button on_press(|| $flag.toggle())\n    button on_press(|| $count.update(|n| *n += 1))\n";
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

    // `overlay` builds an `Overlay` widget (a top-layer portal) and collects its children.
    #[test]
    fn overlay_builds_overlay_widget() {
        let src = "[view]\ncol\n    text \"behind\"\n    overlay align:center justify:center\n        text \"on top\"\n";
        let code = crate::transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains("Overlay::new("),
            "overlay emits an Overlay widget:\n{code}"
        );
        assert!(
            code.contains(".align_items(") && code.contains(".justify_content("),
            "overlay forwards layout attrs for positioning content:\n{code}"
        );
    }

    // A `box` with a `hover(...)` override wires `.on_hover_style(...)`.
    #[test]
    fn box_hover_emits_on_hover_style() {
        let src = "[view]\nbox fill:#101010 hover_style(fill:#f0f0f0 stroke:#ff0000) radius:10\n    text \"x\"\n";
        let out = crate::transpile_source_with_theme(src, "demo", None, None).unwrap();
        assert!(
            out.rust_code.contains(".on_hover_style("),
            "hover(...) should wire on_hover_style:\n{}",
            out.rust_code
        );
    }

    // `on_drag` wires the drag gesture and (on a plain col) forces the StyledContainer upgrade.
    #[test]
    fn on_drag_wires_and_upgrades_container() {
        let src = "[logic]\nlet x = signal(0.0f32);\n[view]\ncol on_drag(|px, _py| $x.set(px))\n    text \"t\"\n";
        let out = crate::transpile_source_with_theme(src, "demo", None, None).unwrap();
        let code = &out.rust_code;
        assert!(
            code.contains("StyledContainer::new("),
            "on_drag upgrades a plain col to a StyledContainer:\n{code}"
        );
        assert!(
            code.contains(".on_drag(") && code.contains("move |px, _py| x.set(px)"),
            "on_drag wires the handler (with the $signal cloned in):\n{code}"
        );
    }

    // A plain `col` gains a hover style, so it must upgrade to a StyledContainer (which has a background).
    #[test]
    fn plain_col_with_hover_upgrades_to_styled_container() {
        let src = "[view]\ncol hover_style(fill:#f0f0f0)\n    text \"x\"\n";
        let out = crate::transpile_source_with_theme(src, "demo", None, None).unwrap();
        let code = &out.rust_code;
        assert!(
            code.contains("StyledContainer::new("),
            "a col with hover should become a StyledContainer:\n{code}"
        );
        assert!(
            code.contains(".on_hover_style("),
            "and wire on_hover_style:\n{code}"
        );
    }

    // A `box` with an `active_style(...)` override wires `.on_active_style(...)` — the pressed-state swap,
    // symmetric with `hover_style`.
    #[test]
    fn box_active_style_emits_on_active_style() {
        let src = "[view]\nbox fill:#101010 active_style(fill:#303030) radius:10\n    text \"x\"\n";
        let out = crate::transpile_source_with_theme(src, "demo", None, None).unwrap();
        assert!(
            out.rust_code.contains(".on_active_style("),
            "active_style(...) should wire on_active_style:\n{}",
            out.rust_code
        );
    }

    // A plain `col` with only an `active_style` must still upgrade to a StyledContainer (needs a background).
    #[test]
    fn plain_col_with_active_style_upgrades_to_styled_container() {
        let src = "[view]\ncol active_style(fill:#303030)\n    text \"x\"\n";
        let out = crate::transpile_source_with_theme(src, "demo", None, None).unwrap();
        let code = &out.rust_code;
        assert!(
            code.contains("StyledContainer::new(") && code.contains(".on_active_style("),
            "a col with only active_style should become a StyledContainer and wire it:\n{code}"
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
            code.contains("card(__slots)?"),
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
            code.contains("CardProps { elevated: true }"),
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

    // A `$`-free closure prop on a component carries its source span (an expr marker) so LSP completion
    // works inside it, exactly like inside an element's `on_press`.
    #[test]
    fn component_free_closure_prop_carries_source_span() {
        let src = "[view]\ncard on_tap(|| toggle())\n    text \"x\"\n";
        let out = crate::transpile_source_with_theme(src, "demo", None, None).unwrap();
        assert!(!out.rust_code.contains("@RSX@"), "markers must be stripped");
        let span = out
            .expr_spans
            .iter()
            .find(|s| &src[s.rsx_start as usize..(s.rsx_start + s.len) as usize] == "|| toggle()")
            .expect("closure prop should emit a source span");
        let gen_frag =
            &out.rust_code[span.gen_start as usize..(span.gen_start + span.len) as usize];
        assert_eq!(
            gen_frag, "|| toggle()",
            "span must map verbatim onto the generated closure"
        );
    }

    fn sig(props_default: bool, fields: &[&str], has_slot: bool) -> crate::ComponentSig {
        crate::ComponentSig {
            has_props: !fields.is_empty(),
            props_default,
            prop_fields: fields.iter().map(|s| s.to_string()).collect(),
            has_slot,
            color_fields: Vec::new(),
            text_fields: Vec::new(),
            optional_fields: Vec::new(),
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
                .contains("card(CardProps { ..Default::default() }, Slots::new())?"),
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
                .contains("DocHeaderProps { title: \"X\", ..Default::default() }"),
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

    // `input` binds `value:$signal` (cloned), builds a size/color text style, forwards layout attrs, and
    // wires an optional `on_submit`.
    #[test]
    fn input_binds_value_style_and_submit() {
        let src = "[logic]\nlet name = signal(String::new());\n[view]\ninput value:$name size:16 color:primary width:200 on_submit(|| $name.set(String::new()))\n";
        let code = crate::transpile_source_with_theme(src, "demo", Some("SandboxTheme"), None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains("Input::new(name.clone(),"),
            "binds the value signal (cloned):\n{code}"
        );
        assert!(
            code.contains("TextStyle::new(16.0, use_theme::<SandboxTheme>().primary())"),
            "size + reactive colour style:\n{code}"
        );
        assert!(
            code.contains(".width(200"),
            "forwards layout attrs:\n{code}"
        );
        assert!(code.contains(".on_submit("), "wires on_submit:\n{code}");
    }

    // A `$`-source `for` with a `key` clause inside a container emits a transparent fragment (source read,
    // key closure, item builder) that reconciles into the parent's node — no boxed `ReactiveList`.
    #[test]
    fn reactive_for_emits_reactive_list() {
        let src = "[logic]\nlet items = signal(vec![1i32, 2, 3]);\n[view]\ncol\n    for n in $items key *n\n        text \"x\"\n";
        let code = crate::transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains("fragment(") && !code.contains("ReactiveList"),
            "a reactive for in a container should be a transparent fragment, not a boxed list:\n{code}"
        );
        assert!(
            code.contains("Container::from_slots("),
            "the host container collects slots:\n{code}"
        );
        assert!(
            code.contains("move || items.get()"),
            "source reads the signal:\n{code}"
        );
        assert!(
            code.contains("|n| *n"),
            "key closure from `key *n`:\n{code}"
        );
        assert!(code.contains("move |n|"), "item builder closure:\n{code}");
    }

    // A reactive `for` without a `key` clause reconciles by position (keyless transparent fragment).
    #[test]
    fn reactive_for_without_key_uses_positional_reconciliation() {
        let src = "[view]\ncol\n    for n in $items\n        text \"x\"\n";
        let code = crate::transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains("fragment_positional("),
            "a keyless reactive for should reconcile by position:\n{code}"
        );
        assert!(
            !code.contains("compile_error!"),
            "a keyless reactive for must compile, not error:\n{code}"
        );
    }

    // The canonical bar case: a reactive `for` between static siblings inside a `row` is transparent — the
    // host `row` collects slots (`from_slots` with its own `flex_row`), the statics become `ChildSlot::stat`,
    // and the fragment is pushed between them, so its items lay out horizontally as real row siblings.
    #[test]
    fn reactive_for_in_row_is_transparent_between_static_siblings() {
        let src = "[logic]\nlet ws = signal(vec![1i32, 2]);\n[view]\nrow\n    text \"L\"\n    for w in $ws key *w\n        text \"x\"\n    text \"R\"\n";
        let code = crate::transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains("Container::from_slots(") && code.contains("flex_row"),
            "the row hosts slots and keeps its row axis:\n{code}"
        );
        assert!(
            code.contains("fragment(") && !code.contains("ReactiveList"),
            "the reactive for is a transparent fragment, not a boxed list:\n{code}"
        );
        assert!(
            code.contains("ChildSlot::stat(") && code.contains("__slots.push("),
            "static siblings and the fragment share the row's slot collection:\n{code}"
        );
    }

    // A reactive `for … gap:N` in a `row` stays transparent: it emits a gap-carrying fragment (spaced by a
    // per-item margin, resolved against the host's axis at runtime), not a boxed `ReactiveList` that would
    // impose its own column. The `row` still hosts slots and keeps its row axis, so the items flow horizontally.
    #[test]
    fn reactive_for_with_gap_in_row_is_transparent_gap_fragment() {
        let src = "[logic]\nlet ws = signal(vec![1i32, 2]);\n[view]\nrow\n    for w in $ws key *w gap:6\n        text \"x\"\n";
        let code = crate::transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains("fragment_gap(") && !code.contains("ReactiveList"),
            "a reactive for with gap in a slot host is a transparent gap fragment, not a boxed list:\n{code}"
        );
        assert!(
            code.contains("(6) as f32"),
            "the gap value is passed to fragment_gap:\n{code}"
        );
        assert!(
            code.contains("Container::from_slots(") && code.contains("flex_row"),
            "the host row keeps its row axis and hosts slots:\n{code}"
        );
    }

    // A keyless reactive `for … gap:N` in a slot host uses the positional gap fragment.
    #[test]
    fn reactive_for_with_gap_keyless_uses_positional_gap_fragment() {
        let src = "[view]\nrow\n    for w in $ws gap:4\n        text \"x\"\n";
        let code = crate::transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains("fragment_positional_gap(") && code.contains("(4) as f32"),
            "a keyless reactive for with gap is a positional gap fragment:\n{code}"
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

    // An `if` whose condition reads a signal (`$`) inside a container becomes a transparent fragment keyed
    // on the bool, whose builder holds the then/else branches — the shown branch's nodes are siblings of the
    // surrounding children, not wrapped in a boxed list.
    #[test]
    fn reactive_if_emits_reactive_list() {
        let src = "[logic]\nlet show = signal(true);\n[view]\ncol\n    if $show\n        text \"yes\"\n    else\n        text \"no\"\n";
        let code = crate::transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains("fragment(") && !code.contains("ReactiveList"),
            "a reactive if in a container should be a transparent fragment:\n{code}"
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

    // A view that is exactly one static `if`/`if-else` returns its chosen branch directly, so the branch's
    // element becomes the component root. An injected `Container::column` wrapper would trap a `row
    // align:stretch` root at content height on the column's main axis, so it must not appear — the branch
    // element is boxed straight as the return value.
    #[test]
    fn root_static_if_returns_branch_directly_without_wrapper() {
        let src =
            "[view]\nif vertical\n    col\n        text \"a\"\nelse\n    row\n        text \"b\"\n";
        let code = crate::transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            !code.contains("Container::column(children![__"),
            "a single-if root must not wrap its branch in an injected column:\n{code}"
        );
        assert!(
            code.contains("Ok(Box::new(__col_0))") && code.contains("Ok(Box::new(__row_0))"),
            "each branch's element is returned directly as the component root:\n{code}"
        );
        assert!(
            code.contains("if vertical {") && code.contains("} else {"),
            "the conditional drives which branch becomes the root:\n{code}"
        );
    }

    // A reactive `for` whose body is a single widget yields it bare, not wrapped in a per-item flex-column that would collapse a stretch chip to its text height.
    #[test]
    fn reactive_for_single_child_item_is_not_wrapped() {
        let src = "[logic]\nlet items = signal(vec![1i32, 2]);\n[view]\nrow align:stretch\n    for n in $items key *n\n        box fill:primary\n            text \"x\"\n";
        let code = crate::transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            !code.contains("Ok(box_item(Container::new(LayoutStyle::new().flex_column()"),
            "a single-widget for item must not be wrapped in a collapsing flex-column cell:\n{code}"
        );
        assert!(
            code.contains("Ok(box_item(__sbox_0))"),
            "the item's styled box is returned bare so the row can stretch it:\n{code}"
        );
    }

    // Declarative transform attrs emit `.with_transform(box_transform(...))`; `scale` fills both axes.
    #[test]
    fn transform_attrs_emit_with_transform() {
        let src = "[view]\ncol\n    box fill:primary rotate:30 scale:1.2\n        text \"x\"\n";
        let code = crate::transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains(".with_transform("),
            "emits with_transform:\n{code}"
        );
        assert!(
            code.contains("box_transform(__r,"),
            "calls the helper:\n{code}"
        );
        assert!(
            code.contains("(30) as f32"),
            "rotate arg cast to f32:\n{code}"
        );
        assert!(
            code.matches("(1.2) as f32").count() >= 2,
            "scale fills both axes:\n{code}"
        );
    }

    // A transform upgrades a plain col/row to a StyledContainer (only it carries `with_transform`).
    #[test]
    fn transform_promotes_plain_container() {
        let src = "[view]\ncol\n    col rotate:5\n        text \"x\"\n";
        let code = crate::transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains("StyledContainer"),
            "promoted to styled:\n{code}"
        );
        assert!(
            code.contains(".with_transform("),
            "carries the transform:\n{code}"
        );
    }

    // A box with no transform attrs emits no transform call.
    #[test]
    fn no_transform_no_call() {
        let src = "[view]\ncol\n    box fill:primary\n        text \"x\"\n";
        let code = crate::transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            !code.contains(".with_transform("),
            "no transform, no call:\n{code}"
        );
    }

    // `on_hover`/`on_key` attributes emit the matching container methods, with signal sugar applied.
    #[test]
    fn event_callbacks_emit_on_hover_and_on_key() {
        // Paren form for both, since `key:value` consumes to end of line (only the last attr can use `:`).
        let src = "[logic]\nlet hot = signal(false);\nlet n = signal(0i32);\n[view]\ncol\n    box fill:primary on_hover(|h| $hot.set(h)) on_key(|_k| $n += 1)\n        text \"x\"\n";
        let code = crate::transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(code.contains(".on_hover("), "emits on_hover:\n{code}");
        assert!(code.contains(".on_key("), "emits on_key:\n{code}");
    }

    // `on_pointer_move` carries the pointer position, so a viewport can answer *where* rather than *whether*.
    #[test]
    fn on_pointer_move_emits_the_container_method() {
        let src = "[logic]\nlet at = signal((0.0f32, 0.0f32));\n[view]\ncol\n    box on_pointer_move(|x, y| $at.set((x, y)))\n        text \"x\"\n";
        let code = crate::transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains(".on_pointer_move("),
            "emits on_pointer_move:\n{code}"
        );
        assert!(
            code.contains("move |x, y| at.set((x, y))"),
            "with the $signal cloned in:\n{code}"
        );
    }

    // `drag_button` widens which buttons may start the box's drag; the primary one always can.
    #[test]
    fn drag_button_emits_the_extra_buttons() {
        let src = "[view]\ncol\n    box drag_button:secondary,auxiliary on_drag(|_x, _y| ())\n        text \"x\"\n";
        let code = crate::transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains(
                ".drag_button(PointerButton::Secondary).drag_button(PointerButton::Auxiliary)"
            ),
            "emits one call per button:\n{code}"
        );
    }

    // An event callback upgrades a plain col/row to a StyledContainer (only it carries the callbacks).
    #[test]
    fn on_hover_promotes_plain_container() {
        let src = "[view]\ncol\n    col on_hover(|_h| ())\n        text \"x\"\n";
        let code = crate::transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains("StyledContainer"),
            "promoted to styled:\n{code}"
        );
        assert!(code.contains(".on_hover("), "carries the callback:\n{code}");
    }

    // `heading` resolves as a component call carrying its `text` (its title style lives in `ui-components`).
    #[test]
    fn heading_resolves_as_widget_component() {
        let src = "[view]\ncol\n    heading text:\"Title\"\n";
        let code = crate::transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains(
                "heading(HeadingProps { text: Box::new(move || \"Title\".to_string()) })"
            ),
            "heading is a component call carrying its text:\n{code}"
        );
    }

    // A `$signal` button colour must be cloned into the reactive colour closure (color_expr drops the `$`,
    // so the clone scan needs the raw fill value) — otherwise reusing the signal elsewhere fails to compile.
    #[test]
    fn btn_signal_color_is_cloned_into_style_closure() {
        let src = "[logic]\nlet c = signal(Color::WHITE);\n[view]\nbutton label:\"x\" fill:$c\n";
        let code = crate::transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains("let c = c.clone();"),
            "signal colour cloned:\n{code}"
        );
        assert!(
            code.contains("c.get()"),
            "read inside the style closure:\n{code}"
        );
    }

    // Without a registry, behavior is unchanged: a childless unknown component is a bare `tag(ctx)?` call
    // (no slot arg, no default), preserving the per-file fallback.
    #[test]
    fn no_registry_keeps_flat_call() {
        let out =
            crate::transpile_source_with_theme("[view]\nmy_card\n", "demo", None, None).unwrap();
        assert!(
            out.rust_code.contains("my_card()?"),
            "without a registry a no-attr call stays flat:\n{}",
            out.rust_code
        );
    }

    // A box `fill(expr)` computes a reactive Color from state: `$signal` reads become reactive `.get()`
    // calls, the signal is cloned into the paint closure (so the outer handle stays usable), and a loop var
    // and helper call are emitted verbatim. This is the state-driven paint a stateful chip/pill needs.
    #[test]
    fn box_fill_computed_expression_is_reactive() {
        let src = "[logic]\nlet snap = signal(0i32);\nlet ids = signal(vec![1i32]);\n[view]\nrow\n    for id in $ids key id\n        box fill(chip_fill($snap, id)) radius:6\n            text \"x\"\n";
        let code = crate::transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains("chip_fill(snap.get(), id)"),
            "the fill expr reads the signal reactively and keeps the loop var verbatim:\n{code}"
        );
        assert!(
            code.contains("let snap = snap.clone();"),
            "the captured signal is cloned into the paint closure:\n{code}"
        );
    }

    // A text `color(expr)` is reactive the same way: `$signal` → reactive read, cloned into the style closure.
    #[test]
    fn text_color_computed_expression_is_reactive() {
        let src =
            "[logic]\nlet snap = signal(0i32);\n[view]\ntext \"hi\" color(text_color($snap))\n";
        let code = crate::transpile_source_with_theme(src, "demo", None, None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains("text_color(snap.get())"),
            "the color expr reads the signal reactively:\n{code}"
        );
        assert!(
            code.contains("let snap = snap.clone();"),
            "the captured signal is cloned into the style closure:\n{code}"
        );
    }

    // A bare color token (no `(`/`$`) must still resolve through the theme, not be swept into the expression
    // arm — guards the computed-expression detection from misfiring on ordinary tokens.
    #[test]
    fn bare_color_token_still_resolves_via_theme() {
        let src = "[view]\nbox fill:primary\n    text \"x\"\n";
        let code = crate::transpile_source_with_theme(src, "demo", Some("MyTheme"), None)
            .unwrap()
            .rust_code;
        assert!(
            code.contains("use_theme::<MyTheme>().primary()"),
            "a bare token resolves via the theme:\n{code}"
        );
    }

    // Every name in `builtin_tags()` must have a real dispatch arm in `emit_element`; a tag missing one
    // silently falls through to `emit_component_call` and is emitted as a bare `tag(ctx…)?` component
    // call (see `no_registry_keeps_flat_call`). Guards the registry table and the dispatch `match`
    // against drift (e.g. `column`, which was listed as a builtin yet handled only under `col`).
    #[test]
    fn every_builtin_tag_has_a_dispatch_arm() {
        for &(tag, _ctor) in crate::registry::builtin_tags() {
            let src = format!("[view]\n{tag}\n");
            // A real emit arm may still reject a bare instance (missing required attr, etc.); that is not
            // a fall-through, since `emit_component_call` returns unconditionally and never errors.
            let Ok(out) = crate::transpile_source_with_theme(&src, "demo", None, None) else {
                continue;
            };
            assert!(
                !out.rust_code.contains(&format!("{tag}()?")),
                "builtin tag `{tag}` fell through to emit_component_call (no dispatch arm):\n{}",
                out.rust_code
            );
        }
    }
}
