//! Control-flow emitters: `if`/`for` blocks and the branch-into-children helper.

use std::fmt::Write;

use rsx_parser::{ForBlock, IfBlock, ViewNode};

use super::signals::{pattern_idents, substitute_reads, wrap_signal_clones};
use super::{ChildEmit, ChildMode, ViewGen, expr_marker, forces_fragment};

impl ViewGen<'_> {
    pub(super) fn emit_if(&mut self, block: &IfBlock) -> ChildEmit {
        // A `$`-signal in the condition makes this a reactive conditional: the shown branch swaps when the
        // condition changes. A plain condition stays a one-shot construction `if` (branch chosen at build).
        if block.condition.contains('$') {
            return self.emit_reactive_if(block);
        }
        let pad = self.indent_str();
        let mut code = String::new();
        // The condition is already trimmed by the parser and emitted verbatim, so its span maps directly.
        let cond = block.condition.trim();
        let marker = expr_marker(block.condition_start, cond.len());
        let _ = writeln!(code, "{pad}if {marker}{cond} {{");
        self.indent += 1;
        self.emit_branch_into_children(&block.then_branch, &mut code);
        self.indent -= 1;

        if let Some(else_branch) = &block.else_branch {
            let _ = writeln!(code, "{pad}}} else {{");
            self.indent += 1;
            self.emit_branch_into_children(else_branch, &mut code);
            self.indent -= 1;
        }
        let _ = write!(code, "{pad}}}");
        ChildEmit::Dynamic { code }
    }

    /// A reactive `if $cond { … } else { … }`. Inside a slot host it is a transparent fragment — the shown
    /// branch's nodes are real siblings of the surrounding children (inheriting the parent's flex direction),
    /// exactly like a reactive `for`. Outside one it falls back to a boxed single-item `ReactiveList`
    /// ([`Self::emit_reactive_if_boxed`]).
    fn emit_reactive_if(&mut self, block: &IfBlock) -> ChildEmit {
        if !self.in_slot_host() {
            return self.emit_reactive_if_boxed(block);
        }
        let var = self.next_variable_name("frag");
        let pad = self.indent_str();
        let cond = block.condition.trim();
        // Source yields a one-element `vec![<bool>]`; the element is the reconciliation key and branch selector.
        let source =
            wrap_signal_clones(&[cond], format!("move || vec![{}]", substitute_reads(cond)));

        let mut code = String::new();
        let _ = writeln!(code, "{pad}let {var} = fragment(");
        let _ = writeln!(code, "{pad}    {source},");
        let _ = writeln!(code, "{pad}    |__cond: &bool| *__cond,");
        let _ = writeln!(
            code,
            "{pad}    move |__cond: bool| -> Result<Box<dyn LayoutItem>, LayoutError> {{"
        );
        self.indent += 2;
        let cell = self.emit_branch_cell(block, &mut code);
        let pad2 = self.indent_str();
        let _ = writeln!(code, "{pad2}Ok(box_item({cell}))");
        self.indent -= 2;
        let _ = writeln!(code, "{pad}    }},");
        let _ = write!(code, "{pad});");
        ChildEmit::Fragment { name: var, code }
    }

    /// The pre-transparency reactive `if`: a single-item `ReactiveList` keyed on the condition boolean (the
    /// old branch's nodes are disposed and the new branch built when it flips). Used where a fragment can't
    /// attach — component-slot children, a bare root, overlay/scroll.
    fn emit_reactive_if_boxed(&mut self, block: &IfBlock) -> ChildEmit {
        let var = self.next_variable_name("node");
        let pad = self.indent_str();
        let cond = block.condition.trim();
        let source =
            wrap_signal_clones(&[cond], format!("move || vec![{}]", substitute_reads(cond)));

        let mut code = String::new();
        let _ = writeln!(code, "{pad}let {var} = ReactiveList::new(");
        let _ = writeln!(code, "{pad}    {source},");
        let _ = writeln!(code, "{pad}    |__cond: &bool| *__cond,");
        let _ = writeln!(
            code,
            "{pad}    move |__cond: bool| -> Result<Box<dyn LayoutItem>, LayoutError> {{"
        );
        let _ = writeln!(
            code,
            "{pad}        let mut __children: Vec<Box<dyn LayoutItem>> = Vec::new();"
        );
        // A local `__children`: push the branches into it, not into any surrounding slot host.
        self.with_child_sink(ChildMode::Vec, |g| {
            let _ = writeln!(code, "{pad}        if __cond {{");
            g.indent += 3;
            g.emit_branch_into_children(&block.then_branch, &mut code);
            g.indent -= 3;
            let _ = writeln!(code, "{pad}        }} else {{");
            if let Some(else_branch) = &block.else_branch {
                g.indent += 3;
                g.emit_branch_into_children(else_branch, &mut code);
                g.indent -= 3;
            }
            let _ = writeln!(code, "{pad}        }}");
        });
        let _ = writeln!(
            code,
            "{pad}        Ok(box_item(Container::new(LayoutStyle::new().flex_column(), __children)?))"
        );
        let _ = writeln!(code, "{pad}    }},");
        let _ = write!(code, "{pad})?;");
        ChildEmit::Simple { name: var, code }
    }

    /// Emits a reactive `if`'s branches into one accumulator (`if __cond { … } else { … }`) and returns the
    /// `Container::{new|from_slots}(flex_column, …)?` cell expression. Slot mode (a `ChildSlot` accumulator)
    /// when either branch holds a reactive fragment; otherwise a `Box<dyn LayoutItem>` vec.
    fn emit_branch_cell(&mut self, block: &IfBlock, code: &mut String) -> String {
        let pad = self.indent_str();
        let slots = block.then_branch.iter().any(forces_fragment)
            || block
                .else_branch
                .as_ref()
                .is_some_and(|e| e.iter().any(forces_fragment));
        let mode = if slots {
            ChildMode::Slots
        } else {
            ChildMode::Vec
        };
        if slots {
            let _ = writeln!(code, "{pad}let mut __slots: Vec<ChildSlot> = Vec::new();");
        } else {
            let _ = writeln!(
                code,
                "{pad}let mut __children: Vec<Box<dyn LayoutItem>> = Vec::new();"
            );
        }
        self.with_child_sink(mode, |g| {
            let _ = writeln!(code, "{pad}if __cond {{");
            g.indent += 1;
            g.emit_branch_into_children(&block.then_branch, code);
            g.indent -= 1;
            let _ = writeln!(code, "{pad}}} else {{");
            if let Some(else_branch) = &block.else_branch {
                g.indent += 1;
                g.emit_branch_into_children(else_branch, code);
                g.indent -= 1;
            }
            let _ = writeln!(code, "{pad}}}");
        });
        let (expr, ctor) = if slots {
            ("__slots", "from_slots")
        } else {
            ("__children", "new")
        };
        format!("Container::{ctor}(LayoutStyle::new().flex_column(), {expr})?")
    }

    pub(super) fn emit_for(&mut self, block: &ForBlock) -> ChildEmit {
        // A `$`-prefixed source is a reactive list: build a keyed, reconciling `ReactiveList` widget
        // instead of a one-shot construction loop.
        if block.iterable.trim_start().starts_with('$') {
            return self.emit_reactive_for(block);
        }
        let pad = self.indent_str();
        let mut code = String::new();
        let _ = writeln!(
            code,
            "{pad}for {} in {} {{",
            block.pattern.trim(),
            block.iterable.trim()
        );
        self.indent += 1;
        // Loop variables are often borrowed (`items.iter()`), but widget closures require `'static` captures; bind owned copies so they can be moved in.
        let body_pad = self.indent_str();
        let idents = pattern_idents(&block.pattern);
        for ident in &idents {
            let _ = writeln!(code, "{body_pad}let {ident} = {ident}.to_owned();");
        }
        let added = idents.len();
        self.loop_variables.extend(idents);
        self.emit_branch_into_children(&block.body, &mut code);
        self.loop_variables
            .truncate(self.loop_variables.len() - added);
        self.indent -= 1;
        let _ = write!(code, "{pad}}}");
        ChildEmit::Dynamic { code }
    }

    /// A reactive `for x in $items [key <expr>] [gap:N]`. Inside a slot host it is a transparent fragment: its
    /// items reconcile straight into the host container's node, so they are real siblings of the static
    /// children and flow in the host's flex direction — a `for` in a `row` is horizontal. A `gap:` keeps that
    /// transparency, laid out as a per-item main-axis margin rather than a wrapper's container gap
    /// (`fragment_gap`/`fragment_positional_gap`). A non-host context (component slot / root / overlay /
    /// scroll) falls back to a boxed `ReactiveList` ([`Self::emit_reactive_for_boxed`]), which carries the gap
    /// on its own node.
    fn emit_reactive_for(&mut self, block: &ForBlock) -> ChildEmit {
        if !self.in_slot_host() {
            return self.emit_reactive_for_boxed(block);
        }
        let var = self.next_variable_name("frag");
        let pad = self.indent_str();
        let iterable = block.iterable.trim();
        let pattern = block.pattern.trim();
        let key_expr = block
            .key_expr
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let gap_expr = block
            .gap_expr
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let source = wrap_signal_clones(
            &[iterable],
            format!("move || {}", substitute_reads(iterable)),
        );
        let ctor = match (key_expr.is_some(), gap_expr.is_some()) {
            (true, false) => "fragment",
            (true, true) => "fragment_gap",
            (false, false) => "fragment_positional",
            (false, true) => "fragment_positional_gap",
        };

        let mut code = String::new();
        let _ = writeln!(code, "{pad}let {var} = {ctor}(");
        let _ = writeln!(code, "{pad}    {source},");
        if let Some(key_expr) = key_expr {
            let _ = writeln!(code, "{pad}    |{pattern}| {key_expr},");
        }
        let _ = writeln!(
            code,
            "{pad}    move |{pattern}| -> Result<Box<dyn LayoutItem>, LayoutError> {{"
        );
        self.indent += 2;
        let idents = pattern_idents(pattern);
        let added = idents.len();
        self.loop_variables.extend(idents);
        let cell = self.emit_item_cell(&block.body, &mut code);
        self.loop_variables
            .truncate(self.loop_variables.len() - added);
        let pad2 = self.indent_str();
        let _ = writeln!(code, "{pad2}Ok(box_item({cell}))");
        self.indent -= 2;
        let _ = writeln!(code, "{pad}    }},");
        if let Some(gap_expr) = gap_expr {
            let _ = writeln!(code, "{pad}    ({gap_expr}) as f32,");
        }
        let _ = write!(code, "{pad});");
        ChildEmit::Fragment { name: var, code }
    }

    /// The pre-transparency reactive `for`: a boxed `ReactiveList` (its own container node) with an optional
    /// `key`/`gap`. Used where a fragment can't attach (a non-slot-host context).
    fn emit_reactive_for_boxed(&mut self, block: &ForBlock) -> ChildEmit {
        let var = self.next_variable_name("node");
        let pad = self.indent_str();
        let iterable = block.iterable.trim();
        let pattern = block.pattern.trim();

        let key_expr = block
            .key_expr
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let gap_expr = block
            .gap_expr
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());

        let constructor = match (key_expr.is_some(), gap_expr.is_some()) {
            (true, false) => "new",
            (true, true) => "with_gap",
            (false, false) => "positional",
            (false, true) => "positional_with_gap",
        };

        let source = wrap_signal_clones(
            &[iterable],
            format!("move || {}", substitute_reads(iterable)),
        );

        let mut code = String::new();
        let _ = writeln!(code, "{pad}let {var} = ReactiveList::{constructor}(");
        let _ = writeln!(code, "{pad}    {source},");
        if let Some(key_expr) = key_expr {
            let _ = writeln!(code, "{pad}    |{pattern}| {key_expr},");
        }
        let _ = writeln!(
            code,
            "{pad}    move |{pattern}| -> Result<Box<dyn LayoutItem>, LayoutError> {{"
        );
        let _ = writeln!(
            code,
            "{pad}        let mut __children: Vec<Box<dyn LayoutItem>> = Vec::new();"
        );
        self.indent += 2;
        let idents = pattern_idents(pattern);
        let added = idents.len();
        self.loop_variables.extend(idents);
        self.with_child_sink(ChildMode::Vec, |g| {
            g.emit_branch_into_children(&block.body, &mut code);
        });
        self.loop_variables
            .truncate(self.loop_variables.len() - added);
        self.indent -= 2;

        let _ = writeln!(
            code,
            "{pad}        Ok(box_item(Container::new(LayoutStyle::new().flex_column(), __children)?))"
        );
        let _ = writeln!(code, "{pad}    }},");
        if let Some(gap_expr) = gap_expr {
            let _ = writeln!(code, "{pad}    ({gap_expr}) as f32,");
        }
        let _ = write!(code, "{pad})?;");
        ChildEmit::Simple { name: var, code }
    }

    /// Emits one reconciled item's body as a flex-column cell, returning the `Container::{new|from_slots}(…)?`
    /// expression (`from_slots` when the body nests its own reactive fragment). Writes any accumulator
    /// decl/pushes into `code`. Loop variables must already be in scope.
    fn emit_item_cell(&mut self, body: &[ViewNode], code: &mut String) -> String {
        let pad = self.indent_str();
        let mode = Self::child_mode(body);
        let emits: Vec<ChildEmit> =
            self.with_child_sink(mode, |g| body.iter().map(|n| g.emit_node(n)).collect());
        let expr = self.emit_children_collection(code, &emits, &pad, mode, &[]);
        let ctor = if mode == ChildMode::Slots {
            "from_slots"
        } else {
            "new"
        };
        format!("Container::{ctor}(LayoutStyle::new().flex_column(), {expr})?")
    }

    /// Emits a control-flow branch's nodes, pushing each into the child accumulator in scope (its shape —
    /// `box_item` vs `ChildSlot::stat` — chosen by the current sink). A nested reactive fragment pushes as a
    /// `ChildSlot::Dynamic`.
    fn emit_branch_into_children(&mut self, nodes: &[ViewNode], code: &mut String) {
        let pad = self.indent_str();
        for node in nodes {
            match self.emit_node(node) {
                ChildEmit::Simple { name, code: c } => {
                    let _ = writeln!(code, "{c}");
                    self.push_static_child(code, &pad, &name);
                }
                ChildEmit::Fragment { name, code: c } => {
                    let _ = writeln!(code, "{c}");
                    self.push_fragment_child(code, &pad, &name);
                }
                ChildEmit::Dynamic { code: c } => {
                    let _ = writeln!(code, "{c}");
                }
            }
        }
    }
}
