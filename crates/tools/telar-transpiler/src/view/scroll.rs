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
        // A `virtual` loop anywhere under this scroll needs its live viewport, and only the closure forms hand
        // one over — so the constructor's shape follows what the subtree asked for. Every other scroll keeps the
        // cheaper form it had.
        let viewport = wants_viewport(&el.children).then(|| "__viewport".to_string());
        let bind = viewport.as_deref().unwrap_or("_");
        let build = |content: &str| match (&keep, &viewport) {
            (Some(key), _) => format!(
                "LayoutScrollArea::new_kept({key}, {style}, |{bind}| Ok(Box::new({content}) as Box<dyn LayoutItem>))?"
            ),
            (None, Some(_)) => format!(
                "LayoutScrollArea::new_with({style}, |{bind}| Ok(Box::new({content}) as Box<dyn LayoutItem>))?"
            ),
            (None, None) => format!("LayoutScrollArea::new({style}, Box::new({content}))?"),
        };
        // The same constructors, with the content built inside the closure so it can read the bound viewport.
        let body_pad = format!("{pad}    ");
        let build_with_body = |body: &str, content: &str| {
            let ctor = match &keep {
                Some(key) => format!("LayoutScrollArea::new_kept({key}, {style}, |{bind}| {{"),
                None => format!("LayoutScrollArea::new_with({style}, |{bind}| {{"),
            };
            format!(
                "{body_pad}{ctor}\n{body}{body_pad}    Ok(Box::new({content}) as Box<dyn LayoutItem>)\n{body_pad}}})?\n"
            )
        };

        // LayoutScrollArea wraps a single content item. A reactive `for`/`if` inside becomes a transparent
        // fragment whose items flow in the wrapping flex-column content (`from_slots`); static control flow
        // uses a `Container::column`; a plain single child needs no wrapper.
        let mode = Self::child_mode(&el.children);
        self.indent += 1;
        let inner_pad = self.indent_str();
        let outer_viewport = std::mem::replace(&mut self.scroll_viewport, viewport.clone());
        let child_emits: Vec<ChildEmit> = self.with_child_sink(mode, |g| {
            el.children.iter().map(|child| g.emit_node(child)).collect()
        });
        self.scroll_viewport = outer_viewport;
        self.indent -= 1;

        // The children's statements are built into their own buffer first, because where they belong depends on
        // whether this scroll bound a viewport: a `VirtualList` among them reads `__viewport`, which exists only
        // inside the constructor's closure, so their code has to go in there rather than above it.
        let mut inner = String::new();
        let content = if mode == ChildMode::Literal {
            let mut names = Vec::new();
            for emit in &child_emits {
                if let ChildEmit::Simple { name, code: c } = emit {
                    let _ = writeln!(inner, "{c}");
                    names.push(name.clone());
                }
            }
            wrap_as_single_content(&names)
        } else {
            let children =
                self.emit_children_collection(&mut inner, &child_emits, &inner_pad, mode, &[]);
            let built = if mode == ChildMode::Slots {
                format!("Container::from_slots(LayoutStyle::new().flex_column(), {children})?")
            } else {
                format!("Container::column({children})?")
            };
            let _ = writeln!(inner, "{inner_pad}let __scroll_content = {built};");
            "__scroll_content".to_string()
        };

        let mut code = String::new();
        let _ = writeln!(code, "{pad}let {var} = {{");
        match &viewport {
            Some(_) => {
                let _ = write!(code, "{}", build_with_body(&inner, &content));
            }
            None => {
                let _ = write!(code, "{inner}");
                let _ = writeln!(code, "{inner_pad}{}", build(&content));
            }
        }
        let _ = write!(code, "{pad}}};");
        ChildEmit::Simple { name: var, code }
    }
}

/// Whether any node in this subtree is a `virtual` loop, so the enclosing `scroll` knows to hand its viewport
/// over. A nested `scroll` is not descended into: its own loops are its own to serve.
fn wants_viewport(nodes: &[telar_parser::ViewNode]) -> bool {
    use telar_parser::ViewNode;
    nodes.iter().any(|node| match node {
        ViewNode::ForBlock(block) => {
            block.virtual_row_height.is_some() || wants_viewport(&block.body)
        }
        ViewNode::IfBlock(block) => {
            wants_viewport(&block.then_branch)
                || block.else_branch.as_deref().is_some_and(wants_viewport)
        }
        ViewNode::MatchBlock(block) => block.arms.iter().any(|arm| wants_viewport(&arm.body)),
        ViewNode::Element(el) => el.tag != "scroll" && wants_viewport(&el.children),
        ViewNode::LetStmt(_) => false,
    })
}
