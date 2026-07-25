//! The `lazy` tag: a subtree deferred until the first time its `when:` condition holds.

use std::fmt::Write;

use rsx_parser::{Element, ViewNode};

use super::signals::{captured_idents, rust_str, substitute_reads};
use super::{ChildEmit, ChildMode, ViewGen, forces_child_vec};

impl ViewGen<'_> {
    pub(super) fn emit_lazy(&mut self, el: &Element) -> ChildEmit {
        let var = self.next_variable_name("lazy");
        let pad = self.indent_str();
        let style = self.make_layout_style("lazy", &el.classes, &el.attributes);

        // `Lazy::new` takes a plain child vec, so a reactive `for`/`if` here stays a boxed `ReactiveList` rather than a transparent fragment: cap the mode at `Vec`, never `Slots` — same as `overlay`.
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
                let raw = attr.value.trim();
                clone_block(&[raw], format!("move || {}", substitute_reads(raw)))
            }
            // Without a condition there is nothing to defer *until*, which is almost certainly a mistake rather than a request to build immediately.
            None => format!(
                "move || compile_error!({})",
                rust_str(
                    "lazy: needs a `when:` condition — the subtree is built the first time it holds"
                )
            ),
        };

        // Each closure clones what it captures inside its own block, so neither moves a binding the rest of the view still needs — including the condition signal, which the two closures both read.
        let mut body = String::new();
        let _ = writeln!(
            body,
            "{pad}    move || -> Result<Vec<Box<dyn LayoutItem>>, LayoutError> {{"
        );
        let children =
            self.emit_children_collection(&mut body, &child_emits, &inner_pad, mode, &[]);
        let _ = writeln!(body, "{inner_pad}Ok({children})");
        let _ = write!(body, "{pad}    }}");

        let raw = subtree_snippets(&el.children);
        let raw_refs: Vec<&str> = raw.iter().map(String::as_str).collect();
        let build = self.clone_block_multiline(&raw_refs, body, &format!("{pad}    "));

        let _ = writeln!(code, "{pad}    Lazy::new(");
        let _ = writeln!(code, "{pad}        {style},");
        let _ = writeln!(code, "{pad}        {visible},");
        let _ = writeln!(code, "{pad}        {build},");
        let _ = writeln!(code, "{pad}    )?");
        let _ = write!(code, "{pad}}};");
        ChildEmit::Simple { name: var, code }
    }

    /// [`clone_block`] for a closure whose body spans lines: the clones go on their own line above it, so the
    /// generated code stays readable and the source map keeps pointing at the right `.rsx` lines.
    fn clone_block_multiline(&self, raw_values: &[&str], closure: String, pad: &str) -> String {
        let idents = captured_idents(raw_values, &self.loop_variables);
        if idents.is_empty() {
            return closure;
        }
        let mut out = String::from("{\n");
        for name in idents {
            let _ = writeln!(out, "{pad}    let {name} = {name}.clone();");
        }
        let _ = write!(out, "{closure}\n{pad}}}");
        out
    }
}

/// Wraps a single-line `move` closure in a block that clones every `$name` it references first.
fn clone_block(raw_values: &[&str], closure: String) -> String {
    let idents = captured_idents(raw_values, &[]);
    if idents.is_empty() {
        return closure;
    }
    let prefix: String = idents
        .iter()
        .map(|s| format!("let {s} = {s}.clone(); "))
        .collect();
    format!("{{ {prefix}{closure} }}")
}

/// Every raw source snippet a subtree contains, still carrying its `$` sigils — attribute values, text
/// content, control-flow conditions and verbatim `let`s. Collected so a `move` closure wrapping that subtree
/// can clone the signals it will reference instead of moving them out of the surrounding view.
fn subtree_snippets(nodes: &[ViewNode]) -> Vec<String> {
    let mut out = Vec::new();
    collect_snippets(nodes, &mut out);
    out
}

fn collect_snippets(nodes: &[ViewNode], out: &mut Vec<String>) {
    for node in nodes {
        match node {
            ViewNode::Element(el) => {
                if let Some(content) = &el.content {
                    out.push(content.clone());
                }
                if let Some(params) = &el.leading_params {
                    out.push(params.clone());
                }
                for attr in &el.attributes {
                    out.push(attr.value.clone());
                }
                collect_snippets(&el.children, out);
            }
            ViewNode::IfBlock(block) => {
                out.push(block.condition.clone());
                collect_snippets(&block.then_branch, out);
                if let Some(else_branch) = &block.else_branch {
                    collect_snippets(else_branch, out);
                }
            }
            ViewNode::ForBlock(block) => {
                out.push(block.iterable.clone());
                if let Some(key) = &block.key_expr {
                    out.push(key.clone());
                }
                if let Some(gap) = &block.gap_expr {
                    out.push(gap.clone());
                }
                collect_snippets(&block.body, out);
            }
            ViewNode::LetStmt(stmt) => out.push(stmt.source.clone()),
        }
    }
}
