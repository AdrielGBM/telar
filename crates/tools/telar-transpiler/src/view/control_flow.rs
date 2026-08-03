//! Control-flow emitters: `if`/`for`/`match` blocks and the branch-into-children helper.

use std::fmt::Write;

use telar_parser::{ForBlock, IfBlock, MatchBlock, ViewNode};

use super::signals::{
    captured_idents, clone_block_multiline, pattern_idents, substitute_reads, subtree_snippets,
    wrap_signal_clones,
};
use super::{ChildEmit, ChildMode, ViewGen, expr_marker};

impl ViewGen<'_> {
    /// The clone prelude a `move` closure holding `body` needs: one `let x = x.clone();` per `$signal` (and
    /// per in-scope loop variable) the subtree reads.
    ///
    /// Without it the closure *moves* those bindings, so a signal read inside a reactive branch stops being
    /// available to the rest of the view — a trap the author never wrote and cannot see in their `.rsx`.
    /// Computed before this block's own pattern idents enter `loop_variables`, since those are the closure's
    /// parameters and exist only inside it.
    fn wrap_branch_closure(&self, body: &[ViewNode], closure: String, pad: &str) -> String {
        let raw = subtree_snippets(body);
        let raw_refs: Vec<&str> = raw.iter().map(String::as_str).collect();
        let idents = captured_idents(&raw_refs, &self.loop_variables);
        clone_block_multiline(&idents, closure, pad)
    }

    /// `match <expr> [as <name>] [key <expr>]`. A `$` in the scrutinee makes it reactive; without one the arm is
    /// chosen once at construction and this is an ordinary Rust `match`.
    pub(super) fn emit_match(&mut self, block: &MatchBlock) -> ChildEmit {
        if block.scrutinee.contains('$') {
            return self.emit_reactive_match(block);
        }
        let pad = self.indent_str();
        let scrutinee = block.scrutinee.trim();
        let marker = expr_marker(block.scrutinee_start, scrutinee.len());
        let mut code = String::new();
        let _ = writeln!(code, "{pad}match {marker}{scrutinee} {{");
        for arm in &block.arms {
            self.indent += 1;
            let apad = self.indent_str();
            let amarker = expr_marker(arm.pattern_start, arm.pattern.len());
            let _ = writeln!(code, "{apad}{amarker}{} => {{", arm.pattern);
            self.indent += 1;
            self.emit_branch_into_children(&arm.body, &mut code);
            self.indent -= 1;
            let _ = writeln!(code, "{apad}}}");
            self.indent -= 1;
        }
        let _ = write!(code, "{pad}}}");
        ChildEmit::Dynamic { code }
    }

    /// A reactive `match $expr`: a one-item reconciled list whose key decides when an arm rebuilds. Unlike a
    /// reactive `if`, the key is not the selector — a variant carries a payload, and keying on that payload's
    /// own identity is what lets an arm keep its widget while its contents change. Without a `key` clause it
    /// reconciles on the variant alone, which rebuilds when the shape changes and not when the payload does.
    fn emit_reactive_match(&mut self, block: &MatchBlock) -> ChildEmit {
        let boxed = !self.in_slot_host();
        let var = self.next_variable_name(if boxed { "node" } else { "frag" });
        let pad = self.indent_str();
        let scrutinee = block.scrutinee.trim();
        let source = wrap_signal_clones(
            &[scrutinee],
            format!("move || vec![{}]", substitute_reads(scrutinee)),
        );

        let binding = block.binding.as_deref().unwrap_or("__value");
        let key_fn = match block.key_expr.as_deref().map(str::trim) {
            Some(key) if !key.is_empty() => {
                format!("|{binding}: &_| {}", substitute_reads(key))
            }
            // The variant alone. `discriminant` is `Hash` whether or not the item's own type is.
            _ => "|__value: &_| ::std::mem::discriminant(__value)".to_string(),
        };

        let mut body = String::new();
        let _ = writeln!(
            body,
            "{pad}    move |__value| -> Result<Box<dyn LayoutItem>, LayoutError> {{"
        );
        self.indent += 2;
        let bpad = self.indent_str();
        let _ = writeln!(body, "{bpad}match __value {{");
        for arm in &block.arms {
            self.indent += 1;
            let apad = self.indent_str();
            let amarker = expr_marker(arm.pattern_start, arm.pattern.len());
            let _ = writeln!(body, "{apad}{amarker}{} => {{", arm.pattern);
            self.indent += 1;
            let arm_body = arm.body.clone();
            let cell = self.in_reactive(|g| g.emit_content_cell(&arm_body, &mut body));
            let ipad = self.indent_str();
            let _ = writeln!(body, "{ipad}Ok(box_item({cell}))");
            self.indent -= 1;
            let _ = writeln!(body, "{apad}}}");
            self.indent -= 1;
        }
        let _ = writeln!(body, "{bpad}}}");
        self.indent -= 2;
        let _ = write!(body, "{pad}    }}");

        let arm_nodes: Vec<ViewNode> = block
            .arms
            .iter()
            .flat_map(|arm| arm.body.iter().cloned())
            .collect();
        let branches = self.wrap_branch_closure(&arm_nodes, body, &format!("{pad}    "));

        let mut code = String::new();
        let opener = if boxed {
            "ReactiveList::new("
        } else {
            "fragment("
        };
        let closer = if boxed { ")?;" } else { ");" };
        let _ = writeln!(code, "{pad}let {var} = {opener}");
        let _ = writeln!(code, "{pad}    {source},");
        let _ = writeln!(code, "{pad}    {key_fn},");
        let _ = writeln!(code, "{branches},");
        let _ = write!(code, "{pad}{closer}");
        match boxed {
            true => ChildEmit::Simple { name: var, code },
            false => ChildEmit::Fragment { name: var, code },
        }
    }

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

        let mut body = String::new();
        let _ = writeln!(
            body,
            "{pad}    move |__cond: bool| -> Result<Box<dyn LayoutItem>, LayoutError> {{"
        );
        self.indent += 2;
        self.in_reactive(|g| g.emit_branch_returns(block, &mut body));
        self.indent -= 2;
        let _ = write!(body, "{pad}    }}");
        let branches =
            self.wrap_branch_closure(&Self::branch_nodes(block), body, &format!("{pad}    "));

        let mut code = String::new();
        let _ = writeln!(code, "{pad}let {var} = fragment(");
        let _ = writeln!(code, "{pad}    {source},");
        let _ = writeln!(code, "{pad}    |__cond: &bool| *__cond,");
        let _ = writeln!(code, "{branches},");
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

        let mut body = String::new();
        let _ = writeln!(
            body,
            "{pad}    move |__cond: bool| -> Result<Box<dyn LayoutItem>, LayoutError> {{"
        );
        self.indent += 2;
        self.in_reactive(|g| g.emit_branch_returns(block, &mut body));
        self.indent -= 2;
        let _ = write!(body, "{pad}    }}");
        let branches =
            self.wrap_branch_closure(&Self::branch_nodes(block), body, &format!("{pad}    "));

        let mut code = String::new();
        let _ = writeln!(code, "{pad}let {var} = ReactiveList::new(");
        let _ = writeln!(code, "{pad}    {source},");
        let _ = writeln!(code, "{pad}    |__cond: &bool| *__cond,");
        let _ = writeln!(code, "{branches},");
        let _ = write!(code, "{pad})?;");
        ChildEmit::Simple { name: var, code }
    }

    /// Both branches of an `if` as one node list — what the branch closure actually contains, and so what its
    /// clone prelude has to be computed from. The condition is deliberately excluded: it lives in the source
    /// closure, which clones it separately.
    fn branch_nodes(block: &IfBlock) -> Vec<ViewNode> {
        let mut nodes = block.then_branch.clone();
        if let Some(else_branch) = &block.else_branch {
            nodes.extend(else_branch.iter().cloned());
        }
        nodes
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

        // Computed before this loop's own pattern idents go into scope: those are the closure's parameters, so cloning them above it would name bindings that do not exist there.
        let prelude_pad = format!("{pad}    ");
        let mut body = String::new();
        let _ = writeln!(
            body,
            "{pad}    move |{pattern}| -> Result<Box<dyn LayoutItem>, LayoutError> {{"
        );
        self.indent += 2;
        let idents = pattern_idents(pattern);
        let added = idents.len();
        self.loop_variables.extend(idents);
        let cell = self.in_reactive(|g| g.emit_content_cell(&block.body, &mut body));
        self.loop_variables
            .truncate(self.loop_variables.len() - added);
        let pad2 = self.indent_str();
        let _ = writeln!(body, "{pad2}Ok(box_item({cell}))");
        self.indent -= 2;
        let _ = write!(body, "{pad}    }}");
        let item_builder = self.wrap_branch_closure(&block.body, body, &prelude_pad);

        let mut code = String::new();
        let _ = writeln!(code, "{pad}let {var} = {ctor}(");
        let _ = writeln!(code, "{pad}    {source},");
        if let Some(key_expr) = key_expr {
            let _ = writeln!(code, "{pad}    |{pattern}| {key_expr},");
        }
        let _ = writeln!(code, "{item_builder},");
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

        let prelude_pad = format!("{pad}    ");
        let mut body = String::new();
        let _ = writeln!(
            body,
            "{pad}    move |{pattern}| -> Result<Box<dyn LayoutItem>, LayoutError> {{"
        );
        self.indent += 2;
        let idents = pattern_idents(pattern);
        let added = idents.len();
        self.loop_variables.extend(idents);
        let cell = self.in_reactive(|g| g.emit_content_cell(&block.body, &mut body));
        self.loop_variables
            .truncate(self.loop_variables.len() - added);
        let ipad = self.indent_str();
        let _ = writeln!(body, "{ipad}Ok(box_item({cell}))");
        self.indent -= 2;
        let _ = write!(body, "{pad}    }}");
        let item_builder = self.wrap_branch_closure(&block.body, body, &prelude_pad);

        let mut code = String::new();
        let _ = writeln!(code, "{pad}let {var} = ReactiveList::{constructor}(");
        let _ = writeln!(code, "{pad}    {source},");
        if let Some(key_expr) = key_expr {
            let _ = writeln!(code, "{pad}    |{pattern}| {key_expr},");
        }
        let _ = writeln!(code, "{item_builder},");
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
