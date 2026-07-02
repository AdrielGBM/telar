//! Container emitters: `col`/`row`/`grid` and `box`, plus the box gradient-paint builder.

use std::collections::HashMap;
use std::fmt::Write;

use rsx_parser::{Attr, Element, ViewNode};

use crate::style::format_f32;

use super::signals::{build_gradient_stops, emit_transition_prelude, has_paint};
use super::{ChildEmit, ViewGen};

impl ViewGen<'_> {
    pub(super) fn emit_container(&mut self, el: &Element) -> ChildEmit {
        let var = self.next_variable_name(&el.tag);
        let pad = self.indent_str();
        let style = self.make_layout_style(&el.tag, &el.classes, &el.attributes);

        // A `col`/`row` with paint (inline or from its class) upgrades to a StyledContainer so it can carry a background like `box`; otherwise it stays a plain Container.
        let pattrs = self.paint_attrs(el);
        let (specs, errors) = self.parse_transitions(el);
        let transitions: HashMap<String, String> = specs.into_iter().collect();
        let mut hoists: Vec<String> = Vec::new();
        let pieces = if has_paint(&pattrs) {
            Some(self.rect_style_pieces(&pattrs, &transitions, &mut hoists))
        } else {
            None
        };

        let has_dynamic = el.children.iter().any(|n| {
            matches!(
                n,
                ViewNode::IfBlock(_) | ViewNode::ForBlock(_) | ViewNode::LetStmt { .. }
            )
        });

        self.indent += 1;
        let inner_pad = self.indent_str();
        let mut child_emits = Vec::new();
        for child in &el.children {
            child_emits.push(self.emit_node(child));
        }
        self.indent -= 1;

        let mut code = String::new();
        let _ = writeln!(code, "{pad}let {var} = {{");

        let children =
            self.emit_children_collection(&mut code, &child_emits, &inner_pad, has_dynamic, &[]);
        emit_transition_prelude(&mut code, &inner_pad, &errors, &hoists);
        match pieces {
            Some((closure, opacity_call)) => {
                let _ = writeln!(
                    code,
                    "{inner_pad}StyledContainer::new(ctx, {style}, {closure}, {children})?{opacity_call}"
                );
            }
            None => {
                let _ = writeln!(code, "{inner_pad}Container::new(ctx, {style}, {children})?");
            }
        }

        let _ = write!(code, "{pad}}};");
        ChildEmit::Simple { name: var, code }
    }

    pub(super) fn emit_box(&mut self, el: &Element) -> ChildEmit {
        let var = self.next_variable_name("box");
        let pad = self.indent_str();
        let layout_style = self.make_layout_style("box", &el.classes, &el.attributes);

        // Paint merges inline attrs with the element's class (inline wins), so a `@card` class can carry fill/stroke/radius/etc. — not only inline `box` attributes. `box` is always styled.
        let pattrs = self.paint_attrs(el);
        let (specs, errors) = self.parse_transitions(el);
        let transitions: HashMap<String, String> = specs.into_iter().collect();
        let mut hoists: Vec<String> = Vec::new();
        let (closure, opacity_call) = self.rect_style_pieces(&pattrs, &transitions, &mut hoists);

        let has_dynamic = el.children.iter().any(|n| {
            matches!(
                n,
                ViewNode::IfBlock(_) | ViewNode::ForBlock(_) | ViewNode::LetStmt { .. }
            )
        });

        self.indent += 1;
        let inner_pad = self.indent_str();
        let mut child_emits = Vec::new();
        for child in &el.children {
            child_emits.push(self.emit_node(child));
        }
        self.indent -= 1;

        let mut code = String::new();
        let _ = writeln!(code, "{pad}let {var} = {{");

        let children =
            self.emit_children_collection(&mut code, &child_emits, &inner_pad, has_dynamic, &[]);
        emit_transition_prelude(&mut code, &inner_pad, &errors, &hoists);
        let _ = writeln!(
            code,
            "{inner_pad}StyledContainer::new(ctx, {layout_style}, {closure}, {children})?{opacity_call}"
        );

        let _ = write!(code, "{pad}}};");
        ChildEmit::Simple { name: var, code }
    }

    /// Builds a `Paint::Gradient(...)` expression for a `box` element, using the closure parameter `r` (the rendered `Bounds`) for absolute gradient points.
    ///
    /// `gradient:horizontal/vertical/diagonal/radial` with `from:` / `to:` (required), optional `mid:` / `mid_pos:`.
    pub(super) fn box_gradient_paint(&self, attrs: &[Attr]) -> Option<String> {
        let direction = attrs.iter().find(|a| a.key == "gradient")?.value.clone();
        let from = attrs
            .iter()
            .find(|a| a.key == "from")
            .map(|a| self.color_expr(&a.value))?;
        let to = attrs
            .iter()
            .find(|a| a.key == "to")
            .map(|a| self.color_expr(&a.value))?;
        let mid = attrs
            .iter()
            .find(|a| a.key == "mid")
            .map(|a| self.color_expr(&a.value));
        let mid_pos = attrs
            .iter()
            .find(|a| a.key == "mid_pos")
            .and_then(|a| a.value.parse::<f32>().ok())
            .unwrap_or(0.5);

        let stops = build_gradient_stops(&from, &to, mid.as_deref(), mid_pos);

        match direction.trim() {
            "horizontal" => Some(format!(
                "Paint::Gradient(Gradient::linear(Point::new(r.x, r.y + r.height * 0.5), Point::new(r.x + r.width, r.y + r.height * 0.5), {stops}))"
            )),
            "vertical" => Some(format!(
                "Paint::Gradient(Gradient::linear(Point::new(r.x + r.width * 0.5, r.y), Point::new(r.x + r.width * 0.5, r.y + r.height), {stops}))"
            )),
            "diagonal" => Some(format!(
                "Paint::Gradient(Gradient::linear(Point::new(r.x, r.y), Point::new(r.x + r.width, r.y + r.height), {stops}))"
            )),
            "radial" => {
                // `gr:N` — explicit pixel radius; default is half the shorter side.
                let radius_expr = attrs
                    .iter()
                    .find(|a| a.key == "gr")
                    .and_then(|a| a.value.parse::<f32>().ok())
                    .map(format_f32)
                    .unwrap_or_else(|| "r.width.min(r.height) * 0.5".to_string());
                Some(format!(
                    "Paint::Gradient(Gradient::radial(Point::new(r.x + r.width * 0.5, r.y + r.height * 0.5), {radius_expr}, {stops}))"
                ))
            }
            _ => None,
        }
    }
}
