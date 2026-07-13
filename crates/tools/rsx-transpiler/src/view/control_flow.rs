//! Control-flow emitters: `if`/`for` blocks and the branch-into-children helper.

use std::fmt::Write;

use rsx_parser::{ForBlock, IfBlock, ViewNode};

use super::signals::{pattern_idents, substitute_reads, wrap_signal_clones};
use super::{ChildEmit, ChildMode, ViewGen, expr_marker};

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

    /// A reactive `if $cond`. Inside a slot host it is a transparent fragment (the shown branch's nodes are
    /// real siblings inheriting the parent's flex direction); outside one it falls back to a boxed
    /// `ReactiveList` ([`Self::emit_reactive_if_boxed`]).
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
        self.emit_branch_returns(block, &mut code);
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
        self.indent += 2;
        self.emit_branch_returns(block, &mut code);
        self.indent -= 2;
        let _ = writeln!(code, "{pad}    }},");
        let _ = write!(code, "{pad})?;");
        ChildEmit::Simple { name: var, code }
    }

    /// Emits a reactive `if`'s branches as per-branch returns, each collapsed by [`Self::emit_content_cell`].
    /// A missing `else` yields an empty column.
    fn emit_branch_returns(&mut self, block: &IfBlock, code: &mut String) {
        let pad = self.indent_str();
        let _ = writeln!(code, "{pad}if __cond {{");
        self.indent += 1;
        let then_cell = self.emit_content_cell(&block.then_branch, code);
        let ipad = self.indent_str();
        let _ = writeln!(code, "{ipad}Ok(box_item({then_cell}))");
        self.indent -= 1;
        let _ = writeln!(code, "{pad}}} else {{");
        self.indent += 1;
        let ipad = self.indent_str();
        match &block.else_branch {
            Some(else_branch) => {
                let else_cell = self.emit_content_cell(else_branch, code);
                let _ = writeln!(code, "{ipad}Ok(box_item({else_cell}))");
            }
            None => {
                let _ = writeln!(code, "{ipad}Ok(box_item(Container::column(children![])?))");
            }
        }
        self.indent -= 1;
        let _ = writeln!(code, "{pad}}}");
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
    /// items reconcile into the host's node as real siblings, flowing in its flex direction (a `for` in a `row`
    /// is horizontal), with `gap:` as a per-item margin. Elsewhere it falls back to a boxed `ReactiveList`.
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
        let cell = self.emit_content_cell(&block.body, &mut code);
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
        self.indent += 2;
        let idents = pattern_idents(pattern);
        let added = idents.len();
        self.loop_variables.extend(idents);
        let cell = self.emit_content_cell(&block.body, &mut code);
        self.loop_variables
            .truncate(self.loop_variables.len() - added);
        let ipad = self.indent_str();
        let _ = writeln!(code, "{ipad}Ok(box_item({cell}))");
        self.indent -= 2;
        let _ = writeln!(code, "{pad}    }},");
        if let Some(gap_expr) = gap_expr {
            let _ = writeln!(code, "{pad}    ({gap_expr}) as f32,");
        }
        let _ = write!(code, "{pad})?;");
        ChildEmit::Simple { name: var, code }
    }

    /// Emits `body` as one content item. A single plain widget is returned bare so its parent (not an
    /// injected `flex_column`, which would trap it at content size on the main axis) decides how it fills;
    /// otherwise the children are grouped in a flex-column cell. Loop variables must already be in scope.
    pub(super) fn emit_content_cell(&mut self, body: &[ViewNode], code: &mut String) -> String {
        let pad = self.indent_str();
        let mode = Self::child_mode(body);
        let emits: Vec<ChildEmit> =
            self.with_child_sink(mode, |g| body.iter().map(|n| g.emit_node(n)).collect());
        if let [ChildEmit::Simple { name, code: c }] = emits.as_slice() {
            let _ = writeln!(code, "{c}");
            return name.clone();
        }
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
