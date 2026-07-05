//! Control-flow emitters: `if`/`for` blocks and the branch-into-children helper.

use std::fmt::Write;

use rsx_parser::{ForBlock, IfBlock, ViewNode};

use super::signals::{pattern_idents, substitute_reads, wrap_signal_clones};
use super::{ChildEmit, ViewGen, expr_marker};

impl ViewGen<'_> {
    pub(super) fn emit_if(&mut self, block: &IfBlock) -> ChildEmit {
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

    /// Emits a `for x in $items key <expr>` reactive list as a `ReactiveList` widget: a source closure
    /// that reads the signal, a key closure for identity, and a per-item builder that wraps the loop body's
    /// widgets in a column. The list re-runs and reconciles (reuse/move/insert/drop) when the source changes.
    fn emit_reactive_for(&mut self, block: &ForBlock) -> ChildEmit {
        let var = self.next_variable_name("node");
        let pad = self.indent_str();
        let iterable = block.iterable.trim();
        let pattern = block.pattern.trim();

        let Some(key_expr) = block
            .key_expr
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        else {
            let msg = format!(
                "a reactive `for {pattern} in {iterable}` needs a `key <expr>` clause so items reconcile by identity"
            );
            let code = format!(
                "{pad}compile_error!(\"{msg}\");\n{pad}let {var} = Container::column(ctx, children![])?;"
            );
            return ChildEmit::Simple { name: var, code };
        };

        // Source: `move || items.get()`, cloning the captured signal so the closure owns a 'static handle.
        let source = wrap_signal_clones(
            &[iterable],
            format!("move || {}", substitute_reads(iterable)),
        );

        let mut code = String::new();
        let _ = writeln!(code, "{pad}let {var} = ReactiveList::new(");
        let _ = writeln!(code, "{pad}    ctx,");
        let _ = writeln!(code, "{pad}    {source},");
        let _ = writeln!(code, "{pad}    |{pattern}| {key_expr},");
        let _ = writeln!(
            code,
            "{pad}    move |ctx: &mut WidgetCtx, {pattern}| -> Result<Box<dyn LayoutItem>, LayoutError> {{"
        );
        let _ = writeln!(
            code,
            "{pad}        let mut __children: Vec<Box<dyn LayoutItem>> = Vec::new();"
        );

        // Emit the loop body two levels deeper (inside the builder closure), pushing each widget to __children.
        self.indent += 2;
        let idents = pattern_idents(pattern);
        let added = idents.len();
        self.loop_variables.extend(idents);
        self.emit_branch_into_children(&block.body, &mut code);
        self.loop_variables
            .truncate(self.loop_variables.len() - added);
        self.indent -= 2;

        let _ = writeln!(
            code,
            "{pad}        Ok(box_item(Container::new(ctx, LayoutStyle::new().flex_column(), __children)?))"
        );
        let _ = writeln!(code, "{pad}    }},");
        let _ = write!(code, "{pad})?;");
        ChildEmit::Simple { name: var, code }
    }

    /// Emits a control-flow branch's nodes, pushing every produced widget into the surrounding `__children` vector.
    fn emit_branch_into_children(&mut self, nodes: &[ViewNode], code: &mut String) {
        let pad = self.indent_str();
        for node in nodes {
            match self.emit_node(node) {
                ChildEmit::Simple { name, code: c } => {
                    let _ = writeln!(code, "{c}");
                    let _ = writeln!(code, "{pad}__children.push(box_item({name}));");
                }
                ChildEmit::Dynamic { code: c } => {
                    let _ = writeln!(code, "{c}");
                }
            }
        }
    }
}
