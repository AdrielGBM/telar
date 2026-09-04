//! The `lazy` tag: a subtree deferred until the first time its `when:` condition holds.

use std::fmt::Write;

use telar_parser::Element;

use super::signals::{
    captured_idents_with, clone_block_multiline, rust_str, substitute_reads, subtree_snippets,
    wrap_signal_clones,
};
use super::{ChildEmit, ChildMode, ViewGen, forces_child_vec};

impl ViewGen<'_> {
    pub(super) fn emit_lazy(&mut self, el: &Element) -> ChildEmit {
        let var = self.next_variable_name("lazy");
        let pad = self.indent_str();
        let style = self.make_layout_style("lazy", &el.classes, &el.attributes);

        // `Lazy::new` takes a plain child vec, so a reactive region here stays a boxed `ReactiveList`.
        let mode = if el.children.iter().any(forces_child_vec) {
            ChildMode::Vec
        } else {
            ChildMode::Literal
        };
        self.indent += 2;
        let inner_pad = self.indent_str();
        let child_emits: Vec<ChildEmit> = self.with_child_sink(mode, |g| {
            el.children.iter().map(|child| g.emit_node(child)).collect()
        });
        self.indent -= 2;

        let mut code = String::new();
        let _ = writeln!(code, "{pad}let {var} = {{");

        let cond = el.attributes.iter().find(|a| a.key == "when");
        let visible = match cond {
            Some(attr) => {
                let raw = attr.value.text();
                let raw = super::redundant_parens(raw.trim()).unwrap_or(raw.trim());
                wrap_signal_clones(&[raw], format!("move || {}", substitute_reads(raw)))
            }
            // Without a condition there is nothing to defer until, which is a mistake rather than a request to build now.
            None => format!(
                "move || compile_error!({})",
                rust_str(
                    "lazy: needs a `when:` condition — the subtree is built the first time it holds"
                )
            ),
        };

        // Each closure clones inside its own block, so neither moves a binding the rest of the view needs — including the condition signal, which both read.
        let mut body = String::new();
        let _ = writeln!(
            body,
            "{pad}        move || -> Result<Vec<Box<dyn LayoutItem>>, LayoutError> {{"
        );
        let children =
            self.emit_children_collection(&mut body, &child_emits, &inner_pad, mode, &[]);
        let _ = writeln!(body, "{inner_pad}Ok({children})");
        let _ = write!(body, "{pad}    }}");

        let raw = subtree_snippets(&el.children);
        let raw_refs: Vec<&str> = raw.iter().map(String::as_str).collect();
        let idents = captured_idents_with(&raw_refs, &self.loop_variables, &self.locals);
        let build = clone_block_multiline(&idents, body, &format!("{pad}        "));

        let _ = writeln!(code, "{pad}    Lazy::new(");
        let _ = writeln!(code, "{pad}        {style},");
        let _ = writeln!(code, "{pad}        {visible},");
        let _ = writeln!(code, "{build},");
        let _ = writeln!(code, "{pad}    )?");
        let _ = write!(code, "{pad}}};");
        ChildEmit::Simple { name: var, code }
    }
}
