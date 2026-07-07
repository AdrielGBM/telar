//! Scroll-area emitter.

use std::fmt::Write;

use rsx_parser::Element;

use super::{ChildEmit, ViewGen, wrap_as_single_content};

impl ViewGen<'_> {
    pub(super) fn emit_scroll(&mut self, el: &Element) -> ChildEmit {
        let var = self.next_variable_name(&el.tag);
        let pad = self.indent_str();
        let style = self.make_layout_style(&el.tag, &el.classes, &el.attributes);

        self.indent += 1;
        let inner_pad = self.indent_str();
        // LayoutScrollArea wraps a single content item; if multiple children exist, wrap them in a column first.
        let mut child_emits = Vec::new();
        for child in &el.children {
            child_emits.push(self.emit_node(child));
        }
        self.indent -= 1;

        let mut code = String::new();
        let _ = writeln!(code, "{pad}let {var} = {{");

        let has_dynamic = child_emits
            .iter()
            .any(|e| matches!(e, ChildEmit::Dynamic { .. }));

        if has_dynamic {
            let children =
                self.emit_children_collection(&mut code, &child_emits, &inner_pad, true, &[]);
            let _ = writeln!(
                code,
                "{inner_pad}let __scroll_content = Container::column({children})?;"
            );
            let _ = writeln!(
                code,
                "{inner_pad}LayoutScrollArea::new({style}, Box::new(__scroll_content))?"
            );
        } else {
            let mut names = Vec::new();
            for emit in &child_emits {
                if let ChildEmit::Simple { name, code: c } = emit {
                    let _ = writeln!(code, "{c}");
                    names.push(name.clone());
                }
            }

            let content = wrap_as_single_content(&names);
            let _ = writeln!(
                code,
                "{inner_pad}LayoutScrollArea::new({style}, Box::new({content}))?"
            );
        }

        let _ = write!(code, "{pad}}};");
        ChildEmit::Simple { name: var, code }
    }
}
