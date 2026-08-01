//! Scroll-area emitter.

use std::fmt::Write;

use telar_parser::Element;

use super::{ChildEmit, ChildMode, ViewGen, wrap_as_single_content};

impl ViewGen<'_> {
    pub(super) fn emit_scroll(&mut self, el: &Element) -> ChildEmit {
        let var = self.next_variable_name(&el.tag);
        let pad = self.indent_str();
        let style = self.make_layout_style(&el.tag, &el.classes, &el.attributes);
        // `keep:"a.key"` names this viewport's position among the things its *surface* keeps, so a remounted
        // tree reopens where the reader left it instead of at the top. Named and opt-in rather than implicit:
        // a scroll emitted inside a `for` would otherwise have every row sharing one position.
        let keep = el
            .attributes
            .iter()
            .find(|attr| attr.key == "keep")
            .map(|attr| format!("\"{}\"", attr.value.trim_matches('"')));
        let build = |content: &str| match &keep {
            Some(key) => format!(
                "LayoutScrollArea::new_kept({key}, {style}, |_| Ok(Box::new({content}) as Box<dyn LayoutItem>))?"
            ),
            None => format!("LayoutScrollArea::new({style}, Box::new({content}))?"),
        };

        // LayoutScrollArea wraps a single content item. A reactive `for`/`if` inside becomes a transparent
        // fragment whose items flow in the wrapping flex-column content (`from_slots`); static control flow
        // uses a `Container::column`; a plain single child needs no wrapper.
        let mode = Self::child_mode(&el.children);
        self.indent += 1;
        let inner_pad = self.indent_str();
        let child_emits: Vec<ChildEmit> = self.with_child_sink(mode, |g| {
            el.children.iter().map(|child| g.emit_node(child)).collect()
        });
        self.indent -= 1;

        let mut code = String::new();
        let _ = writeln!(code, "{pad}let {var} = {{");

        if mode == ChildMode::Literal {
            let mut names = Vec::new();
            for emit in &child_emits {
                if let ChildEmit::Simple { name, code: c } = emit {
                    let _ = writeln!(code, "{c}");
                    names.push(name.clone());
                }
            }
            let content = wrap_as_single_content(&names);
            let _ = writeln!(code, "{inner_pad}{}", build(&content));
        } else {
            let children =
                self.emit_children_collection(&mut code, &child_emits, &inner_pad, mode, &[]);
            let content = if mode == ChildMode::Slots {
                format!("Container::from_slots(LayoutStyle::new().flex_column(), {children})?")
            } else {
                format!("Container::column({children})?")
            };
            let _ = writeln!(code, "{inner_pad}let __scroll_content = {content};");
            let _ = writeln!(code, "{inner_pad}{}", build("__scroll_content"));
        }

        let _ = write!(code, "{pad}}};");
        ChildEmit::Simple { name: var, code }
    }
}
