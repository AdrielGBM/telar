//! Generates the body of the component function from the `[view]` section.

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

use telar_parser::{Attr, Element, IfBlock, StyleClass, ViewNode};

use crate::lexer::contains_ident;
use crate::registry::ValueKind;
pub(crate) use signals::{is_paint_key, rust_str, substitute_reads};
use telar_project::{AssetContext, asset_kind_for_tag};

/// Sentinel comment lines that bracket each view node's generated code with the `.rsx` line it came from. They are emitted into the view body during generation and stripped by [`resolve_source_map`] in the transpiler, which turns them into the per-line origin map. The prefix is deliberately un-generatable by normal codegen so it can never collide with real output.
const SRC_PUSH: &str = "//@RSX@PUSH:";
const SRC_POP: &str = "//@RSX@POP";

/// Inline marker emitted immediately before a verbatim `[view]` Rust expression (interpolation `{expr}`, `if`/`let` expressions, closure attr values). Stripped by [`resolve_source_map`], which records the byte position where the expression begins in the generated body, so the analyzer can map a `.rsx` cursor inside the expression onto the generated Rust. Payload: `<rsx_start>:<len>`, the source byte offset and length of the fragment, which is byte-identical in source and output.
const SRC_EXPR_OPEN: &str = "/*@RSX@EXPR:";
const SRC_EXPR_CLOSE: &str = "@*/";

/// Inline marker emitted immediately before the name a shadow binding declares — the `let x = x.clone();` the clone pass writes so a `move` closure can capture without consuming. Stripped by [`resolve_source_map`], which records where the shadow is declared in the generated body. Payload: the name of the source binding it stands for, which is what a rename has to reach through it. Only the transpiler knows the two are the same symbol; rust-analyzer sees two bindings and is right to.
const SRC_SHADOW_OPEN: &str = "/*@RSX@SHADOW:";

/// Builds an [`SRC_SHADOW_OPEN`] marker for a shadow of the source binding `name`.
pub(super) fn shadow_marker(name: &str) -> String {
    format!("{SRC_SHADOW_OPEN}{name}{SRC_EXPR_CLOSE}")
}

/// Builds an [`SRC_EXPR_OPEN`] marker for a verbatim expression at source byte offset `rsx_start` spanning `len` bytes.
fn expr_marker(rsx_start: usize, len: usize) -> String {
    format!("{SRC_EXPR_OPEN}{rsx_start}:{len}{SRC_EXPR_CLOSE}")
}

/// Wraps `emit`'s code in `SRC_PUSH`/`SRC_POP` markers carrying the 0-based `.rsx` `line`.
fn wrap_source_markers(emit: ChildEmit, line: usize) -> ChildEmit {
    let src0 = line.saturating_sub(1);
    emit.map_code(|code| format!("{SRC_PUSH}{src0}\n{code}\n{SRC_POP}"))
}

/// The resolved view body: real lines (each with its origin `.rsx` line) plus the byte spans of the verbatim Rust expressions it contains.
pub(crate) struct ResolvedView {
    pub lines: Vec<(String, Option<u32>)>,
    /// Per expression: `(byte offset within the streamed body, rsx_start, len)`. The streamed body is the lines joined with `\n` (each line followed by a newline), matching how `transpile` appends them, so a caller adds the body's start offset in the final file to get the generated offset.
    pub expr_spans: Vec<(usize, u32, u32)>,
    /// Per shadow binding: `(byte offset within the streamed body of the name it declares, the source binding it stands for)`.
    pub shadows: Vec<(usize, String)>,
}

/// Strips the source markers from a generated view body, returning each real line paired with the `.rsx` line it originated from (a stack tracks nesting, so a node's own lines map to itself and its children's lines map to the children), the verbatim-expression byte spans, and the shadow bindings. Lines outside any marker (root boilerplate) map to `None`.
pub(crate) fn resolve_source_map(marked: &str) -> ResolvedView {
    let mut stack: Vec<u32> = Vec::new();
    let mut lines = Vec::new();
    let mut expr_spans = Vec::new();
    let mut shadows = Vec::new();
    let mut body_len = 0usize;
    for line in marked.split('\n') {
        if let Some(rest) = line.strip_prefix(SRC_PUSH) {
            if let Ok(n) = rest.parse::<u32>() {
                stack.push(n);
            }
        } else if line == SRC_POP {
            stack.pop();
        } else {
            let stripped = strip_markers(line, body_len);
            expr_spans.extend(stripped.expr_spans);
            shadows.extend(stripped.shadows);
            body_len += stripped.clean.len() + 1;
            lines.push((stripped.clean, stack.last().copied()));
        }
    }
    ResolvedView {
        lines,
        expr_spans,
        shadows,
    }
}

/// Removes the inline markers from a single output `line`, returning the cleaned line plus what each marker described. `base` is the body byte offset of this line's start; a marker sits exactly where the thing it describes begins, so that thing's offset is `base + <cleaned bytes emitted so far>`.
/// One line with its markers removed, alongside what each of them described.
struct StrippedLine {
    clean: String,
    expr_spans: Vec<(usize, u32, u32)>,
    shadows: Vec<(usize, String)>,
}

fn strip_markers(line: &str, base: usize) -> StrippedLine {
    let mut out = String::with_capacity(line.len());
    let mut spans = Vec::new();
    let mut shadows = Vec::new();
    let mut rest = line;
    loop {
        // Whichever comes first, so the running offset of the cleaned text stays right for both kinds.
        let (open, is_expr) = match (rest.find(SRC_EXPR_OPEN), rest.find(SRC_SHADOW_OPEN)) {
            (Some(expr), Some(shadow)) => match expr < shadow {
                true => (expr, true),
                false => (shadow, false),
            },
            (Some(expr), None) => (expr, true),
            (None, Some(shadow)) => (shadow, false),
            (None, None) => break,
        };
        out.push_str(&rest[..open]);
        let opener = match is_expr {
            true => SRC_EXPR_OPEN,
            false => SRC_SHADOW_OPEN,
        };
        let after_open = &rest[open + opener.len()..];
        let Some(close) = after_open.find(SRC_EXPR_CLOSE) else {
            out.push_str(rest);
            return StrippedLine {
                clean: out,
                expr_spans: spans,
                shadows,
            };
        };
        let payload = &after_open[..close];
        match is_expr {
            true => {
                if let Some((rsx_start, len)) = payload.split_once(':')
                    && let (Ok(rsx_start), Ok(len)) = (rsx_start.parse::<u32>(), len.parse::<u32>())
                {
                    spans.push((base + out.len(), rsx_start, len));
                }
            }
            false => shadows.push((base + out.len(), payload.to_string())),
        }
        rest = &after_open[close + SRC_EXPR_CLOSE.len()..];
    }
    out.push_str(rest);
    StrippedLine {
        clean: out,
        expr_spans: spans,
        shadows,
    }
}

/// How a container collects its children: a `children![...]` literal (all static, no control flow), a `Vec<Box<dyn LayoutItem>>` mutated by static control flow, or a `Vec<ChildSlot>` when a reactive fragment is among the siblings (so it and the statics reconcile into the same node — the transparent `for`/`if`).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChildMode {
    Literal,
    Vec,
    Slots,
}

/// The child accumulator currently in scope: the `Vec` variable that `if`/`for` bodies push into, and whether it holds `ChildSlot`s (slot mode) vs `Box<dyn LayoutItem>` (vec mode).
pub(crate) struct ChildSink {
    var: &'static str,
    slots: bool,
}

/// Whether a node forces its parent into slot mode: a *reactive* `for`/`if` (a `$`-driven source) becomes a transparent fragment, which only the `ChildSlot`/`from_slots` path can host. A static `for`/`if` does not — it pushes its widgets directly, so a `Vec` accumulator suffices.
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
    /// A reactive `for`/`if` region bound to `name` as a `ChildSlot::Dynamic` (a transparent fragment that reconciles into the host container's node). Forces the parent to collect `ChildSlot`s and build via `from_slots`, so the region's items are real siblings of the static children.
    Fragment { name: String, code: String },
}

impl ChildEmit {
    /// Rewrites the generated code, keeping whatever kind of child this is.
    fn map_code(self, f: impl FnOnce(String) -> String) -> Self {
        match self {
            ChildEmit::Simple { name, code } => ChildEmit::Simple {
                name,
                code: f(code),
            },
            ChildEmit::Dynamic { code } => ChildEmit::Dynamic { code: f(code) },
            ChildEmit::Fragment { name, code } => ChildEmit::Fragment {
                name,
                code: f(code),
            },
        }
    }
}

/// Collapses a set of already-bound widget `names` into a single content expression: an empty column for none, the lone widget for one, or a `Container::column` wrapping all of them for several. Shared by `generate_root` (which boxes the result as the view's return value) and `emit_scroll`'s static branch (`LayoutScrollArea` takes a single content item).
fn wrap_as_single_content(names: &[String]) -> String {
    match names {
        [] => "Container::column(children![])?".to_string(),
        [only] => only.clone(),
        _ => format!("Container::column(children![{}])?", names.join(", ")),
    }
}

/// Emits Rust for a view tree, carrying the scope every node is generated against.
pub struct ViewGen<'a> {
    /// Declared style classes, used to validate class references in elements.
    classes: &'a [StyleClass],
    /// Per-widget-type variable counters, keyed by the descriptive prefix.
    counters: HashMap<String, usize>,
    /// When set, the view binds `theme` and each `[style]` class function binds its own, so `$theme.field` is a read.
    theme_type: Option<String>,
    /// Identifiers the `[logic]` zone binds. A bare name in the view resolves to one of these before the theme is consulted, so a local shadows a same-named token rather than the other way round — see [`crate::signal_scan::scan_locals`].
    locals: Vec<String>,
    /// Names the `[logic]` zone bound to a `signal(…)` or `memo(…)`. Read to tell a genuinely static iterable from one that reads reactive state without the `$` that would make the loop follow it — see [`ViewGen::signal_named_in`].
    signals: Vec<String>,
    /// Indentation depth (in 4-space units) for the current emission scope.
    indent: usize,
    /// Loop-variable identifiers currently in scope, cloned per closure like signals.
    loop_variables: Vec<String>,
    /// Monotonic counter for the hoisted `__transition_N` animation handles.
    transition_count: usize,
    /// The package's baked asset artifact, which a static `svg`/`img` `src:"path"` resolves against. `None` (e.g. an in-memory transpile with no package to anchor it) makes such a `src:` yield a `compile_error!`.
    assets: Option<&'a AssetContext>,
    /// Signatures of every component in the workspace, so `emit_component_call` emits optional props and the slot argument correctly. `None` falls back to the per-file heuristic. Stack of child accumulators (see [`ChildSink`]); the top is the one an emitted `if`/`for` body pushes into. Pushed before a container's children are emitted, popped after.
    child_sinks: Vec<ChildSink>,
    /// `[logic]` bindings a lone-identifier prop names more than once in this view, so the first call site cannot move what the second still needs. Computed once from the whole view in [`Self::generate_root`].
    multi_referenced: Vec<String>,
    /// How many reactive branch/item closures enclose the node being emitted. Non-zero means the code being generated may run more than once for the same content, which is what makes a one-shot `widget` reference unsound there — see [`ViewGen::in_reactive_region`].
    reactive_depth: usize,
    /// Whether each enclosing container lays its children out horizontally. A boxed reactive region reads the top to build its own node the same way round — an `if`/`for` inside a `row` runs horizontally.
    host_rows: Vec<bool>,
    /// The in-scope binding holding the enclosing scroll's live viewport, when the node being emitted sits inside one that exposed it. `VirtualList` needs it to know which rows are on screen, and only a `scroll` can supply it — so a `virtual` loop outside one is an error rather than a surprise at runtime.
    scroll_viewport: Option<String>,
}

impl<'a> ViewGen<'a> {
    pub fn with_theme(
        classes: &'a [StyleClass],
        theme_type: Option<&str>,
        assets: Option<&'a AssetContext>,
    ) -> Self {
        Self {
            classes,
            counters: HashMap::new(),
            theme_type: theme_type.map(str::to_string),
            locals: Vec::new(),
            signals: Vec::new(),
            indent: 1,
            loop_variables: Vec::new(),
            transition_count: 0,
            assets,
            child_sinks: Vec::new(),
            host_rows: Vec::new(),
            multi_referenced: Vec::new(),
            reactive_depth: 0,
            scroll_viewport: None,
        }
    }

    /// Whether the node being emitted sits inside a reactive branch or item closure, i.e. inside code that may build the same content again after the region has disposed it. Whether a lone `[logic]` binding named here must be cloned rather than moved: either the code around it can run again, or another call site still needs it.
    pub(super) fn must_clone_local(&self, name: &str) -> bool {
        self.in_reactive_region() || self.multi_referenced.iter().any(|n| n == name)
    }

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

    /// Runs `emit` with a child-accumulator context in scope, so `if`/`for` bodies emitted inside push into the right `Vec` in the right shape. No sink is pushed for [`ChildMode::Literal`] (nothing pushes).
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

    /// Emits a push of static widget `name` into the current child accumulator, in its shape (a bare `box_item` for a vec sink, wrapped in `ChildSlot::stat` for a slot sink).
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

    /// Whether the child accumulator in scope hosts `ChildSlot`s — i.e. a reactive `for`/`if` here can be a transparent fragment. Outside a slot host (component-slot children, a bare root, overlay/scroll) it must fall back to a boxed `ReactiveList`.
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

    /// Attaches the names the `[logic]` zone binds, so a bare identifier in the view reaches them before the theme. A preview has no logic zone and so passes none.
    pub(crate) fn with_locals(mut self, locals: Vec<String>) -> Self {
        self.locals = locals;
        self
    }

    pub(crate) fn with_signals(mut self, signals: Vec<String>) -> Self {
        self.signals = signals;
        self
    }

    /// Whether `name` is a binding the logic zone made, which a bare reference in the view means before any same-named theme token.
    pub(super) fn is_local(&self, name: &str) -> bool {
        self.locals.iter().any(|local| local == name)
    }

    /// The first signal `code` mentions by name, skipping strings and comments.
    ///
    /// Names, not types: `[logic]` is spliced through verbatim and never type-checked here, so the only thing this can honestly say is *this text refers to something the author declared reactive*. Enough for the one question it is asked — whether an expression that looks static is reading state that moves.
    pub(super) fn signal_named_in(&self, code: &str) -> Option<&str> {
        self.signals
            .iter()
            .find(|name| contains_ident(code, name))
            .map(String::as_str)
    }

    fn next_variable_name(&mut self, tag: &str) -> String {
        let prefix = match asset_kind_for_tag(tag) {
            Some(kind) => kind.var_prefix,
            None => match tag {
                "text" => "text",
                "col" => "col",
                "row" => "row",
                "box" => "sbox",
                "overlay" => "overlay",
                "lazy" => "lazy",
                "input" => "input",
                "path" => "path",
                "canvas" => "canvas",
                _ => "node",
            },
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
        self.multi_referenced = self
            .locals
            .iter()
            .filter(|name| {
                signals::subtree_snippets(nodes)
                    .iter()
                    .filter(|s| s.trim() == name.as_str())
                    .count()
                    > 1
            })
            .cloned()
            .collect();
        // Returned directly so the branch is the component root: an injected column would trap a `row align:stretch` at content height. A reactive condition keeps the fragment-swap path below.
        if let [ViewNode::IfBlock(block)] = nodes
            && !block.condition.contains('$')
        {
            return self.generate_root_if(block);
        }

        let mut out = String::new();
        let mode = Self::child_mode(nodes);

        // Roots of a fragment or static control flow have no container to attach to, so wrap them in one.
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

    /// Generates the body for a view that is a single static `if`/`if-else`: each branch returns its content directly (via [`Self::emit_content_cell`]), so the chosen branch is the component root with no wrapping column. A missing `else` returns an empty column. Source markers/spans are preserved for cursor mapping.
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
            // The generated file is what a diagnostic points at, so a note is worth as much there as in the `.rsx`.
            ViewNode::Comment(text) => ChildEmit::Dynamic {
                code: format!("{}{text}", self.indent_str()),
            },
        };
        let emit = match node {
            ViewNode::Element(el) => self.clip_tail(el, emit),
            _ => emit,
        };
        // Nested nodes nest their own markers; a `let` has no line of its own and inherits the enclosing node's.
        match node {
            ViewNode::Element(el) => wrap_source_markers(emit, el.line),
            ViewNode::IfBlock(block) => wrap_source_markers(emit, block.line),
            ViewNode::ForBlock(block) => wrap_source_markers(emit, block.line),
            ViewNode::MatchBlock(block) => wrap_source_markers(emit, block.line),
            ViewNode::LetStmt(_) | ViewNode::Comment(_) => emit,
        }
    }

    /// Wraps a node's widget in a [`ClippedItem`] when it carries `clip`, cutting its rendered output to the shape the value names.
    ///
    /// `clip` on its own is `Clip::both()`; a value is a `Clip` expression, so `clip:Clip::x()` cuts the left and right edges only — the thing CSS cannot say, since `overflow: hidden` on one axis forces the other out of `visible` — and `clip:(Clip::both().rounded(8.0))` follows a rounded box's own corners. The value used to be an axis keyword from a closed set of three, which is why a rounded box's clipped child cut its corners square and the markup had no word for the radius the renderer already took.
    fn clip_tail(&mut self, el: &Element, emit: ChildEmit) -> ChildEmit {
        let Some(attr) = el.attributes.iter().find(|a| a.key == "clip") else {
            return emit;
        };
        let shape = match attr.value.text().trim() {
            "" => "Clip::both()".to_string(),
            expr => redundant_parens(expr).unwrap_or(expr).to_string(),
        };
        match emit {
            ChildEmit::Simple { name, code } => {
                let pad = self.indent_str();
                let wrapped = self.next_variable_name("clipped");
                let code = format!(
                    "{code}\n{pad}let {wrapped} = ClippedItem::new(box_item({name}), {shape});"
                );
                ChildEmit::Simple {
                    name: wrapped,
                    code,
                }
            }
            // A control-flow block has no attributes and a fragment no single node, so neither is reachable.
            other => other,
        }
    }

    /// Names any attribute a built-in tag does not accept, instead of mapping it to a builder call that does nothing. It checks the same table the analyzer completes from, so an attribute the editor never suggests is now one the build refuses too — before this, `cols:` on a plain `box` compiled and did nothing. Component tags are exempt: their keys are `Props` fields, which rustc already checks.
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

    /// Names any attribute whose value its key cannot mean, instead of dropping the property or quietly substituting a default for it.
    ///
    /// The counterpart to [`Self::unknown_attr_errors`]: that one rejects a key the tag does not have, this one rejects a value the key does not take. Between them the language stops being strict about keys and silent about values — before this, `align:centre` laid out unaligned and `fit:covr` fitted the other way, both with nothing to point at.
    fn value_errors(&self, el: &Element) -> String {
        if !crate::registry::is_builtin_tag(&el.tag) {
            return String::new();
        }
        let pad = self.indent_str();
        el.attributes
            .iter()
            .filter_map(|attr| self.attr_value_error(&el.tag, attr))
            .map(|message| format!("{pad}compile_error!({});\n", rust_str(&message)))
            .collect()
    }

    /// Why `attr`'s value is not one its key takes, or `None` when it is (or when the key takes anything).
    fn attr_value_error(&self, tag: &str, attr: &Attr) -> Option<String> {
        let value = attr.value.text().trim();
        let Some(kind) = crate::registry::value_kind(tag, &attr.key) else {
            return match crate::style::layout_prop_call(&attr.key, value) {
                crate::style::PropCall::Invalid(message) => Some(message),
                _ => None,
            };
        };
        match kind {
            // Only a flag key carries the empty spelling, so a valueless `align` is caught here too.
            ValueKind::Keywords(table) => crate::style::keyword(&attr.key, value, table).err(),
            // A number is the axis itself and the names are steps of it, so either spelling is the same value.
            ValueKind::KeywordsOrNumber(table) => (value.parse::<f32>().is_err())
                .then(|| crate::style::keyword(&attr.key, value, table).err())
                .flatten(),
            ValueKind::Number => crate::style::format_number(value).err(),
            // A yes or a no is whatever rustc can make one of, so there is nothing here to refuse.
            ValueKind::Boolean => None,
            ValueKind::Edges => value
                .split_whitespace()
                .find_map(|token| crate::style::format_number(token).err()),
            ValueKind::Color => crate::style::color(value).err(),
        }
    }

    fn emit_element(&mut self, el: &Element) -> ChildEmit {
        let errors = format!("{}{}", self.unknown_attr_errors(el), self.value_errors(el));
        let emit = self.emit_element_inner(el);
        if errors.is_empty() {
            return emit;
        }
        emit.map_code(|code| format!("{errors}{code}"))
    }

    fn emit_element_inner(&mut self, el: &Element) -> ChildEmit {
        if let Some(kind) = asset_kind_for_tag(&el.tag) {
            return match kind.id {
                "svg" => self.emit_svg(el),
                "image" => self.emit_image(el),
                _ => unreachable!("asset kind ids are svg and image"),
            };
        }
        match el.tag.as_str() {
            "text" => self.emit_text(el),
            "col" | "row" | "grid" => self.emit_container(el),
            "box" => self.emit_box(el),
            "overlay" => self.emit_overlay(el),
            "lazy" => self.emit_lazy(el),
            "input" => self.emit_input(el),
            "path" => self.emit_path(el),
            "canvas" => self.emit_canvas(el),
            "scroll" => self.emit_scroll(el),
            "children" => self.emit_slot(el),
            other => self.emit_component_call(el, other),
        }
    }
}

/// Whether a view node must build a mutable `__children` vec rather than a `children![...]` literal: control flow (`if`/`for`/`let`) that mutates the vec in place, or a `children` slot placeholder that splices a runtime `Vec` into it.
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
#[path = "view_test.rs"]
mod tests;

/// The expression inside a redundant paren pair, or `None` when the parens carry meaning.
///
/// `key(expr)` is how the markup delimits a value that would otherwise run to end of line, so the parens are punctuation rather than grammar and emitting them warns `unused_parens` in code the author cannot edit. A top-level comma makes them a tuple, which is grammar, and stays — except in a closure, whose parameter list has a comma of its own.
pub(crate) fn redundant_parens(expr: &str) -> Option<&str> {
    let inner = expr.trim().strip_prefix('(')?.strip_suffix(')')?;
    let is_closure = inner.starts_with('|') || inner.trim_start().starts_with("move |");
    let mut depth = 0i32;
    for c in inner.chars() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' if depth == 0 => return None,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 && !is_closure => return None,
            _ => {}
        }
    }
    Some(inner)
}
