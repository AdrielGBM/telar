//! Control-flow emitters: `if`/`for`/`match` blocks and the branch-into-children helper.

use std::fmt::Write;

use telar_parser::{ForBlock, IfBlock, MatchBlock, ViewNode, VirtualClause};

use super::signals::{RECLONE, ScopedSnippet, pattern_idents, scoped_snippets, substitute_reads};
use super::{ChildEmit, ChildMode, ViewGen, expr_marker};

impl ViewGen<'_> {
    fn with_bindings<R>(&mut self, names: Vec<String>, emit: impl FnOnce(&mut Self) -> R) -> R {
        let added = names.len();
        self.loop_variables.extend(names);
        let result = emit(self);
        self.loop_variables
            .truncate(self.loop_variables.len() - added);
        result
    }

    fn with_row_signals<R>(&mut self, names: Vec<String>, emit: impl FnOnce(&mut Self) -> R) -> R {
        let added = names.len();
        self.signals.extend(names);
        let result = emit(self);
        self.signals.truncate(self.signals.len() - added);
        result
    }

    fn with_loop_bindings<R>(
        &mut self,
        names: Vec<String>,
        emit: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let added = names.len();
        self.loop_bound.extend(names.iter().cloned());
        self.loop_depth += 1;
        let result = self.with_bindings(names, emit);
        self.loop_depth -= 1;
        self.loop_bound.truncate(self.loop_bound.len() - added);
        result
    }

    /// A region of the view written with no `$`, whose shape the run picks: `source` is worked out once through `reads_state`, and if that read no state the region is built in place, its children spliced into the parent as a plain `if`, `match` or `for` builds them; if it read state the same build becomes what a fragment (or, outside a slot host, a boxed list) rebuilds whenever `source` changes. `body` emits the plain form into the parent's sink, once, inside the closure both shapes run.
    fn emit_followed_region(
        &mut self,
        node: ViewNode,
        source: RegionSource<'_>,
        body: impl FnOnce(&mut Self, &mut String),
    ) -> ChildEmit {
        self.in_reactive(|g| {
            let pad = g.indent_str();
            let (sink, slots) = match g.child_sinks.last() {
                Some(sink) => (sink.var, sink.slots),
                None => ("__slots", true),
            };
            let mut code = String::new();
            let parts = g.region_parts(&node, source, sink, slots, body, &mut code);
            let RegionParts { build, source, key } = parts;
            let id = build.trim_end_matches("_build");
            let boxed = !slots;
            let axis = if g.host_is_row() {
                "flex_row"
            } else {
                "flex_column"
            };
            let opener = if boxed {
                "ReactiveList::new("
            } else {
                "fragment_branches("
            };
            let branch = match boxed {
                true => format!("move |_| branch_cell(LayoutStyle::new().{axis}(), {build}()?)"),
                false => format!("move |_| {build}()"),
            };
            let closer = g.boxed_list_closer(boxed);
            let push = match slots {
                true => format!("{sink}.push({id});"),
                false => format!("{sink}.push(box_item({id}));"),
            };
            let _ = writeln!(code, "{pad}if reads_state(&{source}).1 {{");
            let _ = writeln!(code, "{pad}    let {id} = {opener}");
            let _ = writeln!(code, "{pad}        {source},");
            let _ = writeln!(code, "{pad}        {key},");
            let _ = writeln!(code, "{pad}        {branch},");
            let _ = writeln!(code, "{pad}        0.0,");
            let _ = writeln!(code, "{pad}    {closer}");
            let _ = writeln!(code, "{pad}    {push}");
            let _ = writeln!(code, "{pad}}} else {{");
            let _ = writeln!(code, "{pad}    {sink}.extend({build}()?);");
            let _ = write!(code, "{pad}}}");
            ChildEmit::Dynamic { code }
        })
    }

    /// A root `if` written with no `$`: the root is the chosen branch's element when the condition read no state, as a plain root `if` returns it, and a column rebuilding it when it did.
    pub(super) fn emit_followed_root(&mut self, block: &IfBlock) -> String {
        self.with_child_sink(ChildMode::Slots, |g| {
            g.in_reactive(|g| {
                let pad = g.indent_str();
                let mut code = String::new();
                let node = ViewNode::IfBlock(block.clone());
                let parts = g.region_parts(
                    &node,
                    RegionSource::of_if(block),
                    "__slots",
                    true,
                    |g, out| g.emit_plain_if(block, out),
                    &mut code,
                );
                let RegionParts { build, source, key } = parts;
                let _ = writeln!(code, "{pad}if reads_state(&{source}).1 {{");
                let _ = writeln!(
                    code,
                    "{pad}    let __root = fragment_branches({source}, {key}, move |_| {build}(), 0.0);"
                );
                let _ = writeln!(
                    code,
                    "{pad}    Ok(Box::new(Container::from_slots(LayoutStyle::new().flex_column(), vec![__root])?))"
                );
                let _ = writeln!(code, "{pad}}} else {{");
                let _ = writeln!(
                    code,
                    "{pad}    branch_cell(LayoutStyle::new().flex_column(), {build}()?)"
                );
                let _ = write!(code, "{pad}}}");
                code
            })
        })
    }

    /// Writes a region's build and source closures into `code`, bound to names of their own, and answers those names and the key its rebuilds go by.
    fn region_parts(
        &mut self,
        node: &ViewNode,
        source: RegionSource<'_>,
        sink: &str,
        slots: bool,
        body: impl FnOnce(&mut Self, &mut String),
        code: &mut String,
    ) -> RegionParts {
        let pad = self.indent_str();
        let id = self.next_variable_name("node");
        let item = if slots {
            "ChildSlot"
        } else {
            "Box<dyn LayoutItem>"
        };
        let mut built = String::new();
        let _ = writeln!(
            built,
            "{pad}    move || -> Result<Vec<{item}>, LayoutError> {{ {RECLONE}"
        );
        self.indent += 2;
        let ipad = self.indent_str();
        let _ = writeln!(built, "{ipad}let mut {sink}: Vec<{item}> = Vec::new();");
        body(self, &mut built);
        let _ = writeln!(built);
        let _ = writeln!(built, "{ipad}Ok({sink})");
        self.indent -= 2;
        let _ = write!(built, "{pad}    }}");
        let snippets = scoped_snippets(std::slice::from_ref(node), &[]);
        let build = self.owning_rerun(&snippets, built, &format!("{pad}    "));
        let (raw, closure, key) = match source {
            RegionSource::Truth(condition) => (
                condition,
                format!(
                    "move || vec![{}]",
                    substitute_reads(condition, self.theme_access)
                ),
                "|__truth: &bool| *__truth",
            ),
            RegionSource::Runs { raw, read } => {
                let read = substitute_reads(&read, self.theme_access);
                // A borrow moves nothing, so only an `if let` reading its bindings by value needs its own copies.
                let reclone = if read.starts_with("&(") { "" } else { RECLONE };
                (
                    raw,
                    format!(
                        "{{ let __runs = ::std::cell::Cell::new(0u64); move || {{ {reclone}let _ = {read}; __runs.set(__runs.get() + 1); vec![__runs.get()] }} }}"
                    ),
                    "|__run: &u64| *__run",
                )
            }
        };
        let source = self.owning_rerun(
            &[ScopedSnippet::unscoped(raw)],
            closure,
            &format!("{pad}    "),
        );
        let _ = writeln!(code, "{pad}let {id}_build =\n{build};");
        let _ = writeln!(code, "{pad}let {id}_source =\n{source};");
        RegionParts {
            build: format!("{id}_build"),
            source: format!("{id}_source"),
            key,
        }
    }

    /// `if`/`else` as a plain branch, built once where it stands into the sink in scope.
    fn emit_plain_if(&mut self, block: &IfBlock, code: &mut String) {
        let pad = self.indent_str();
        let cond = block.condition.trim();
        let marker = expr_marker(block.condition_start, cond.len());
        let read = match cond.contains('$') {
            true => substitute_reads(cond, self.theme_access),
            false => format!("{marker}{cond}"),
        };
        let _ = writeln!(code, "{pad}if {read} {{");
        self.indent += 1;
        let bound = crate::rust::condition_bindings(cond).unwrap_or_default();
        self.with_bindings(bound, |g| {
            g.emit_branch_into_children(&block.then_branch, code)
        });
        self.indent -= 1;
        if let Some(else_branch) = &block.else_branch {
            let _ = writeln!(code, "{pad}}} else {{");
            self.indent += 1;
            self.emit_branch_into_children(else_branch, code);
            self.indent -= 1;
        }
        let _ = write!(code, "{pad}}}");
    }

    /// `match <expr> [as <name>] [key <expr>]`. A `$` in the scrutinee makes it a reactive match keyed as it says; without one it is a region rebuilt whenever what the scrutinee read changes.
    pub(super) fn emit_match(&mut self, block: &MatchBlock) -> ChildEmit {
        if block.scrutinee.contains('$') {
            return self.emit_reactive_match(block);
        }
        let scrutinee = block.scrutinee.trim();
        self.emit_followed_region(
            ViewNode::MatchBlock(block.clone()),
            RegionSource::Runs {
                raw: scrutinee,
                read: format!("&({scrutinee})"),
            },
            |g, code| g.emit_plain_match(block, code),
        )
    }

    /// `match` as a plain choice of arm, built once where it stands into the sink in scope.
    fn emit_plain_match(&mut self, block: &MatchBlock, code: &mut String) {
        let pad = self.indent_str();
        let scrutinee = block.scrutinee.trim();
        let marker = expr_marker(block.scrutinee_start, scrutinee.len());
        let _ = writeln!(code, "{pad}match {marker}{scrutinee} {{");
        for arm in &block.arms {
            self.indent += 1;
            let apad = self.indent_str();
            let amarker = expr_marker(arm.pattern_start, arm.pattern.len());
            let _ = writeln!(code, "{apad}{amarker}{} => {{", arm.pattern);
            self.indent += 1;
            let bound = crate::rust::arm_bindings(&arm.pattern).unwrap_or_default();
            self.with_bindings(bound, |g| g.emit_branch_into_children(&arm.body, code));
            self.indent -= 1;
            let _ = writeln!(code, "{apad}}}");
            self.indent -= 1;
        }
        let _ = write!(code, "{pad}}}");
    }

    /// A reactive `match $expr`: a one-item reconciled list whose key decides when an arm rebuilds. Unlike a reactive `if`, the key is not the selector — a variant carries a payload, and keying on that payload's own identity is what lets an arm keep its widget while its contents change. Without a `key` clause it reconciles on the variant alone, which rebuilds when the shape changes and not when the payload does.
    fn emit_reactive_match(&mut self, block: &MatchBlock) -> ChildEmit {
        let boxed = !self.in_slot_host();
        let var = self.next_variable_name("node");
        let pad = self.indent_str();
        let scrutinee = block.scrutinee.trim();
        let source = self.owning(
            &[scrutinee],
            &format!(
                "move || vec![{}]",
                substitute_reads(scrutinee, self.theme_access)
            ),
        );

        let binding = block.binding.as_deref().unwrap_or("__value");
        let key_fn = match block.key_expr.as_deref().map(str::trim) {
            Some(key) if !key.is_empty() => {
                format!(
                    "|{binding}: &_| {}",
                    substitute_reads(key, self.theme_access)
                )
            }
            // The variant alone: `discriminant` is `Hash` whether or not the item's own type is.
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

        let arm_snippets: Vec<ScopedSnippet> = block
            .arms
            .iter()
            .flat_map(|arm| scoped_snippets(&arm.body, &pattern_idents(&arm.pattern)))
            .collect();
        let branches = self.owning_subtree(&arm_snippets, body, &format!("{pad}    "));

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
        let _ = writeln!(code, "{pad}    0.0,");
        let _ = write!(code, "{pad}{closer}");
        match boxed {
            true => ChildEmit::Simple { name: var, code },
            false => ChildEmit::Fragment { name: var, code },
        }
    }

    pub(super) fn emit_if(&mut self, block: &IfBlock) -> ChildEmit {
        self.emit_followed_region(
            ViewNode::IfBlock(block.clone()),
            RegionSource::of_if(block),
            |g, code| g.emit_plain_if(block, code),
        )
    }

    pub(super) fn emit_for(&mut self, block: &ForBlock) -> ChildEmit {
        // A `$`-prefixed source builds a keyed, reconciling `ReactiveList` instead of a construction loop.
        if block.iterable.trim_start().starts_with('$') {
            if let Some(clause) = &block.virtual_clause {
                return self.emit_virtual_for(block, clause);
            }
            return self.emit_reactive_for(block);
        }
        let pad = self.indent_str();
        let mut refused = String::new();
        // Each clause names something only a reconciled list has. Dropping them in silence read as "my key is being honoured" right up until a row went wrong.
        for (clause, why) in [
            (
                block.key_expr.as_ref().map(|_| "key"),
                "a `key` names an identity for reconciling rows one by one, which a loop over a reading (`$items`) does — a loop over anything else is built as a whole",
            ),
            (
                block.gap_expr.as_ref().map(|_| "gap"),
                "a `gap:` on a `for` spaces *reconciled* rows, which a loop over a reading (`$items`) has — put the gap on the enclosing container instead",
            ),
            (
                block.virtual_clause.as_ref().map(|_| "virtual"),
                "`virtual` builds only the rows a viewport shows as it scrolls, out of a reading (`$items`) it can ask for any row of",
            ),
        ] {
            if let Some(name) = clause {
                let message = format!("`for … {name}` has no effect here: {why}");
                let _ = writeln!(refused, "{pad}compile_error!({message:?});");
            }
        }
        let iterable = block.iterable.trim();
        let region = self.emit_followed_region(
            ViewNode::ForBlock(block.clone()),
            RegionSource::Runs {
                raw: iterable,
                read: format!("&({iterable})"),
            },
            |g, code| g.emit_plain_for(block, code),
        );
        match region {
            ChildEmit::Dynamic { code } => ChildEmit::Dynamic {
                code: format!("{refused}{code}"),
            },
            other => other,
        }
    }

    /// `for` as a construction loop, built once where it stands into the sink in scope.
    fn emit_plain_for(&mut self, block: &ForBlock, code: &mut String) {
        let pad = self.indent_str();
        let iterable = block.iterable.trim();
        let iterated = self
            .by_value(iterable, iterable.to_string())
            .unwrap_or_else(|| iterable.to_string());
        let _ = writeln!(code, "{pad}for {} in {iterated} {{", block.pattern.trim());
        self.indent += 1;
        // Loop variables are often borrowed, but widget closures need `'static` captures, so bind owned copies.
        let body_pad = self.indent_str();
        let idents = pattern_idents(&block.pattern);
        for ident in &idents {
            let _ = writeln!(code, "{body_pad}let {ident} = {ident}.to_owned();");
        }
        self.with_loop_bindings(idents, |g| g.emit_branch_into_children(&block.body, code));
        self.indent -= 1;
        let _ = write!(code, "{pad}}}");
    }

    /// A reactive `for x in $items [key <expr>] [gap:N]`. Inside a slot host it is a transparent fragment: its items reconcile into the host's node as real siblings, flowing in its flex direction (a `for` in a `row` is horizontal), with `gap:` as a per-item margin. Elsewhere it is a boxed `ReactiveList` owning its own container node.
    ///
    /// With a `key`, each row is built from its item by value and rebuilt when its key changes. Without one, the row's index is its key, so a row stays where it is while the items under it move or change: its pattern binds a signal of the item now at that index, which the body reads as `$x`.
    fn emit_reactive_for(&mut self, block: &ForBlock) -> ChildEmit {
        let pad = self.indent_str();
        let iterable = block.iterable.trim();
        let pattern = block.pattern.trim();
        let key_expr = match row_key(block, &pad) {
            Ok(key_expr) => key_expr,
            Err(refused) => return refused,
        };
        let boxed = !self.in_slot_host();
        let var = self.next_variable_name("node");
        let gap_expr = block
            .gap_expr
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let source = self.owning(
            &[iterable],
            &format!("move || {}", substitute_reads(iterable, self.theme_access)),
        );
        let ctor = match (boxed, key_expr.is_some()) {
            (false, true) => "fragment",
            (false, false) => "fragment_positional",
            (true, true) => "ReactiveList::new",
            (true, false) => "ReactiveList::positional",
        };
        let item_builder = self.emit_item_closure(block, pattern, key_expr.is_some(), &pad);
        let closer = self.boxed_list_closer(boxed);

        let mut code = String::new();
        let _ = writeln!(code, "{pad}let {var} = {ctor}(");
        let _ = writeln!(code, "{pad}    {source},");
        if let Some(key_expr) = key_expr {
            let _ = writeln!(code, "{pad}    |{pattern}| {key_expr},");
        }
        let _ = writeln!(code, "{item_builder},");
        match gap_expr {
            Some(gap_expr) => {
                let _ = writeln!(code, "{pad}    ({gap_expr}) as f32,");
            }
            None => {
                let _ = writeln!(code, "{pad}    0.0,");
            }
        }
        let _ = write!(code, "{pad}{closer}");
        match boxed {
            true => ChildEmit::Simple { name: var, code },
            false => ChildEmit::Fragment { name: var, code },
        }
    }

    /// The `move |params| -> Result<…>` row closure a reactive `for` hands its list constructor, with the clone prelude its body reads.
    ///
    /// The body is walked with this loop's own pattern idents already bound: those are the closure's parameters, so a `$rect` under `for (m, rect) in $chips` names one of them and cloning it above the closure would name a binding that does not exist there. A keyless loop's pattern binds the row's item signal, so the body is walked with it counted among the signals.
    fn emit_item_closure(
        &mut self,
        block: &ForBlock,
        params: &str,
        keyed: bool,
        pad: &str,
    ) -> String {
        let mut body = String::new();
        let _ = writeln!(
            body,
            "{pad}    move |{params}| -> Result<Box<dyn LayoutItem>, LayoutError> {{"
        );
        self.indent += 2;
        let idents = pattern_idents(block.pattern.trim());
        let snippets = scoped_snippets(&block.body, &idents);
        let row_signals = match keyed {
            true => Vec::new(),
            false => idents.clone(),
        };
        let cell = self.with_row_signals(row_signals, |g| {
            g.with_loop_bindings(idents, |g| {
                g.in_reactive(|g| g.emit_content_cell(&block.body, &mut body))
            })
        });
        let ipad = self.indent_str();
        let _ = writeln!(body, "{ipad}Ok(box_item({cell}))");
        self.indent -= 2;
        let _ = write!(body, "{pad}    }}");
        self.owning_subtree(&snippets, body, &format!("{pad}    "))
    }

    /// A `virtual` loop: only the rows the enclosing scroll shows get built, instead of every row up front.
    ///
    /// Always boxed — a `VirtualList` owns the container its rows scroll inside, so it cannot be a transparent fragment. `row_height:` is the fixed-height `VirtualList::new`; `estimated_height:` is `VirtualList::measured` over a `VirtualRows` made here, which `rows:$handle` also stores in a signal for the view to `reveal` with. Without a `key` they are `VirtualList::positional` and `VirtualList::measured_positional`, whose rows bind a signal of their item as a keyless reactive `for`'s do.
    fn emit_virtual_for(&mut self, block: &ForBlock, clause: &VirtualClause) -> ChildEmit {
        let pad = self.indent_str();
        let Some(viewport) = self.scroll_viewport.clone() else {
            return ChildEmit::Dynamic {
                code: format!(
                    "{pad}compile_error!({:?});",
                    "a `virtual` loop needs an enclosing `scroll`: it builds only the rows that viewport shows, and outside one there is no viewport to ask"
                ),
            };
        };
        let iterable = block.iterable.trim();
        let pattern = block.pattern.trim();
        let key_expr = match row_key(block, &pad) {
            Ok(key_expr) => key_expr,
            Err(refused) => return refused,
        };
        let var = self.next_variable_name("node");

        let source = self.owning(
            &[iterable],
            &format!("move || {}", substitute_reads(iterable, self.theme_access)),
        );

        // `VirtualList` hands a row its index alongside the item; the author's pattern binds the item, `__index` the index.
        let item_builder = self.emit_item_closure(
            block,
            &format!("__index: usize, {pattern}"),
            key_expr.is_some(),
            &pad,
        );
        let (fixed, measured) = match key_expr {
            Some(_) => ("VirtualList::new", "VirtualList::measured"),
            None => (
                "VirtualList::positional",
                "VirtualList::measured_positional",
            ),
        };

        let mut code = String::new();
        match (&clause.row_height, &clause.estimated_height) {
            (Some(row_height), _) => {
                let _ = writeln!(code, "{pad}let {var} = {fixed}(");
                let _ = writeln!(code, "{pad}    LayoutStyle::new().flex_column(),");
                let _ = writeln!(code, "{pad}    {viewport}.clone(),");
                let _ = writeln!(code, "{pad}    ({row_height}) as f32,");
                let _ = writeln!(code, "{pad}    2,");
            }
            (None, Some(estimate)) => {
                let rows = self.next_variable_name("rows");
                let _ = writeln!(
                    code,
                    "{pad}let {rows} = VirtualRows::new({viewport}.clone(), ({estimate}) as f32);"
                );
                if let Some(handle) = &clause.rows {
                    let handle = handle.trim_start_matches('$');
                    let _ = writeln!(code, "{pad}{handle}.set(Some({rows}.clone()));");
                }
                let overscan = clause.overscan.as_ref().map_or_else(
                    || format!("({estimate}) as f32 * 2.0"),
                    |px| format!("({px}) as f32"),
                );
                let _ = writeln!(code, "{pad}let {var} = {measured}(");
                let _ = writeln!(code, "{pad}    LayoutStyle::new().flex_column(),");
                let _ = writeln!(code, "{pad}    {rows},");
                let _ = writeln!(code, "{pad}    {overscan},");
            }
            (None, None) => {
                return ChildEmit::Dynamic {
                    code: format!(
                        "{pad}compile_error!({:?});",
                        "a `virtual` loop needs `row_height:` or `estimated_height:`"
                    ),
                };
            }
        }
        let _ = writeln!(code, "{pad}    {source},");
        if let Some(key_expr) = key_expr {
            let _ = writeln!(code, "{pad}    |{pattern}| {key_expr},");
        }
        let _ = writeln!(code, "{item_builder},");
        let _ = write!(code, "{pad})?;");
        ChildEmit::Simple { name: var, code }
    }

    /// The tail that closes a reactive list constructor. A fragment attaches to its host and just ends; a boxed `ReactiveList` returns a `Result` and turns into a row when it sits inside one — every constructor builds a column, which is right everywhere except here, since a reactive region that owns a node of its own inherits nothing from the container it is about to be attached to.
    fn boxed_list_closer(&self, boxed: bool) -> String {
        match boxed {
            true if self.host_is_row() => ")?.as_row();".to_string(),
            true => ")?;".to_string(),
            false => ");".to_string(),
        }
    }

    /// Emits `body` as one content item. A single plain widget is returned bare so its parent (not an injected `flex_column`, which would trap it at content size on the main axis) decides how it fills; otherwise the children are grouped in a flex-column cell. Loop variables must already be in scope.
    pub(super) fn emit_content_cell(&mut self, body: &[ViewNode], code: &mut String) -> String {
        let pad = self.indent_str();
        let mode = Self::child_mode(body);
        let emits: Vec<ChildEmit> =
            self.with_child_sink(mode, |g| body.iter().map(|n| g.emit_node(n)).collect());
        // A comment is a note in the generated file, never a child, so it cannot turn a lone widget into a cell around it.
        let (notes, widgets): (Vec<_>, Vec<_>) = body
            .iter()
            .zip(&emits)
            .partition(|(node, _)| matches!(node, ViewNode::Comment(_)));
        if let [(_, ChildEmit::Simple { name, code: c })] = widgets.as_slice() {
            for (_, note) in &notes {
                if let ChildEmit::Dynamic { code: note } = note {
                    let _ = writeln!(code, "{note}");
                }
            }
            let _ = writeln!(code, "{c}");
            return name.clone();
        }
        let expr = self.emit_children_collection(code, &emits, &pad, mode, &[]);
        let ctor = if mode == ChildMode::Slots {
            "from_slots"
        } else {
            "new"
        };
        // Follows the host's axis, or a branch with several children inside a `row` would stack and overflow it.
        let axis = if self.host_is_row() {
            "flex_row"
        } else {
            "flex_column"
        };
        format!("Container::{ctor}(LayoutStyle::new().{axis}(), {expr})?")
    }

    /// Emits a control-flow branch's nodes, pushing each into the child accumulator in scope (its shape — `box_item` vs `ChildSlot::stat` — chosen by the current sink). A nested reactive fragment pushes as a `ChildSlot::Dynamic`.
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

/// A reactive or `virtual` loop's `key` expression, `None` for a keyless loop, or the `compile_error!` refusing a keyless loop whose pattern destructures: without a key each row binds a signal of the item at its index, and a signal cannot be taken apart.
fn row_key<'a>(block: &'a ForBlock, pad: &str) -> Result<Option<&'a str>, ChildEmit> {
    let key_expr = block
        .key_expr
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let pattern = block.pattern.trim();
    if key_expr.is_some() || pattern == "_" || telar_project::is_plain_identifier(pattern) {
        return Ok(key_expr);
    }
    Err(ChildEmit::Dynamic {
        code: {
            let message = format!(
                "`for {pattern} in {}` has no `key`, so each row is bound to a signal of the item at its index, and a signal cannot be destructured: bind one name and read its parts through it (`$item.0`), or add a `key` to build each row from its item by value",
                block.iterable.trim()
            );
            format!("{pad}compile_error!({message:?});")
        },
    })
}

/// What a region whose shape the run picks reads, and so when it is rebuilt.
pub(super) enum RegionSource<'a> {
    /// A plain condition, rebuilt when it flips.
    Truth(&'a str),
    /// What a form that binds reads, rebuilt whenever that changes, since what it bound may have: `raw` is the expression as written, `read` what evaluates it.
    Runs { raw: &'a str, read: String },
}

impl<'a> RegionSource<'a> {
    pub(super) fn of_if(block: &'a IfBlock) -> Self {
        let condition = block.condition.trim();
        match crate::rust::condition_bindings(condition) {
            None => Self::Truth(condition),
            Some(_) => Self::Runs {
                raw: condition,
                read: format!("if {condition} {{ true }} else {{ false }}"),
            },
        }
    }
}

/// The names [`ViewGen::region_parts`] bound a region's closures to, and the key its rebuilds go by.
struct RegionParts {
    build: String,
    source: String,
    key: &'static str,
}
