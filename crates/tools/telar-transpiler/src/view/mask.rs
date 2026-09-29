//! The `mask` tag: its first child the shape its second is seen through.

use std::fmt::Write;

use telar_parser::{Element, ViewNode};

use super::signals::rust_str;
use super::{ChildEmit, ChildMode, ViewGen};

impl ViewGen<'_> {
    pub(super) fn emit_mask(&mut self, el: &Element) -> ChildEmit {
        let var = self.next_variable_name("mask");
        let pad = self.indent_str();
        let style = self.make_layout_style("mask", &el.classes, &el.attributes);
        let children: Vec<&ViewNode> = el
            .children
            .iter()
            .filter(|child| !matches!(child, ViewNode::Comment(_)))
            .collect();
        let refused = |why: &str| ChildEmit::Simple {
            name: var.clone(),
            code: format!("{pad}::core::compile_error!({});", rust_str(why)),
        };
        let [source, content] = children.as_slice() else {
            return refused(
                "a `mask` holds two children: the source it is seen through, then the content",
            );
        };
        if !matches!(source, ViewNode::Element(_)) || !matches!(content, ViewNode::Element(_)) {
            return refused("a `mask`'s source and content are elements, not control flow");
        }
        self.indent += 1;
        let emits: Vec<ChildEmit> = self.with_child_sink(ChildMode::Literal, |g| {
            [*source, *content]
                .into_iter()
                .map(|child| g.emit_node(child))
                .collect()
        });
        self.indent -= 1;
        let [
            ChildEmit::Simple {
                name: source,
                code: source_code,
            },
            ChildEmit::Simple {
                name: content,
                code: content_code,
            },
        ] = emits.as_slice()
        else {
            return refused("a `mask`'s source and content are elements, not control flow");
        };
        let mut code = String::new();
        let _ = writeln!(code, "{pad}let {var} = {{");
        let _ = writeln!(code, "{source_code}");
        let _ = writeln!(code, "{content_code}");
        let _ = writeln!(
            code,
            "{pad}    Mask::new({style}, box_item({source}), box_item({content}))?"
        );
        let _ = write!(code, "{pad}}};");
        ChildEmit::Simple { name: var, code }
    }
}
