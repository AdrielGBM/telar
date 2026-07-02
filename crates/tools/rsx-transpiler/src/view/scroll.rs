//! Scroll-area emitter.

use std::fmt::Write;

use rsx_parser::Element;

use super::{ChildEmit, ViewGen};

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
            let _ = writeln!(
                code,
                "{inner_pad}let mut __children: Vec<Box<dyn LayoutItem>> = Vec::new();"
            );
            for emit in &child_emits {
                match emit {
                    ChildEmit::Simple { name, code: c } => {
                        let _ = writeln!(code, "{c}");
                        let _ = writeln!(code, "{inner_pad}__children.push(box_item({name}));");
                    }
                    ChildEmit::Dynamic { code: c } => {
                        let _ = writeln!(code, "{c}");
                    }
                }
            }
            let _ = writeln!(
                code,
                "{inner_pad}let __scroll_content = Container::column(ctx, __children)?;"
            );
            let _ = writeln!(
                code,
                "{inner_pad}LayoutScrollArea::new(ctx, {style}, Box::new(__scroll_content))?"
            );
        } else {
            let mut names = Vec::new();
            for emit in &child_emits {
                if let ChildEmit::Simple { name, code: c } = emit {
                    let _ = writeln!(code, "{c}");
                    names.push(name.clone());
                }
            }

            let content = match names.len() {
                0 => "Container::column(ctx, children![])?".to_string(),
                1 => names.remove(0),
                _ => {
                    let items = names.join(", ");
                    format!("Container::column(ctx, children![{items}])?")
                }
            };

            let _ = writeln!(
                code,
                "{inner_pad}LayoutScrollArea::new(ctx, {style}, Box::new({content}))?"
            );
        }

        let _ = write!(code, "{pad}}};");
        ChildEmit::Simple { name: var, code }
    }
}
