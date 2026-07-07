//! Canvas emitter and its render-node children (`rect`, `text`, `line`, `layer`).

use std::fmt::Write;

use rsx_parser::{Attr, Element, ViewNode};

use crate::style::format_f32;

use super::signals::{
    build_gradient_stops, build_rect_style, canvas_param_bindings, rust_str, wrap_signal_clones,
};
use super::{ChildEmit, ViewGen};

impl ViewGen<'_> {
    pub(super) fn emit_canvas(&mut self, el: &Element) -> ChildEmit {
        let var = self.next_variable_name(&el.tag);
        let pad = self.indent_str();
        let style = self.make_layout_style(&el.tag, &el.classes, &el.attributes);
        let inner = format!("{pad}    ");

        let canvas_children: Vec<&Element> = el
            .children
            .iter()
            .filter_map(|n| {
                if let ViewNode::Element(e) = n {
                    Some(e)
                } else {
                    None
                }
            })
            .collect();

        let mut body = String::new();
        if canvas_children.is_empty() {
            // Legacy behaviour: explicit (w, h) param bindings or empty stub.
            let params = el.leading_params.as_deref().unwrap_or("");
            let bindings = canvas_param_bindings(params, &pad);
            body.push_str(&bindings);
            let _ = writeln!(body, "{inner}RenderNode::group([])");
        } else {
            // Inject dimension locals so children can write w:full / h:full.
            let _ = writeln!(body, "{inner}let __w = __rect.width;");
            let _ = writeln!(body, "{inner}let __h = __rect.height;");
            let exprs: Vec<String> = canvas_children
                .iter()
                .map(|child| self.emit_render_node_expr(child))
                .collect();
            if exprs.len() == 1 {
                let _ = writeln!(body, "{inner}{}", exprs[0]);
            } else {
                let _ = writeln!(body, "{inner}RenderNode::group([");
                for expr in &exprs {
                    let _ = writeln!(body, "{inner}    {expr},");
                }
                let _ = writeln!(body, "{inner}])");
            }
        }

        // A `$ident` colour anywhere in the subtree (rect fill/stroke/gradient stops, line/text color, shadow color) already resolves through `color_expr`'s `.get()` branch (see `emit_canvas_rect`/`emit_canvas_line`/`emit_canvas_text`); clone every such signal into the shared draw closure up front so the outer binding stays usable elsewhere, mirroring `rect_style_pieces`' `wrap_signal_clones` for a styled container.
        let mut raw_colors: Vec<&str> = Vec::new();
        for child in &canvas_children {
            collect_canvas_colors(child, &mut raw_colors);
        }
        let closure = wrap_signal_clones(&raw_colors, format!("move |__rect| {{\n{body}{pad}}}"));

        let code = format!("{pad}let {var} = Canvas::new({style}, {closure})?;");
        ChildEmit::Simple { name: var, code }
    }

    /// Emits a `RenderNode` expression for an element that is a direct child of a `canvas` element. The result is an expression string, not a statement.
    fn emit_render_node_expr(&self, el: &Element) -> String {
        match el.tag.as_str() {
            "rect" => self.emit_canvas_rect(el),
            "text" => self.emit_canvas_text(el),
            "line" => self.emit_canvas_line(el),
            "layer" => self.emit_canvas_layer(el),
            other => format!("/* unsupported canvas child: {other} */"),
        }
    }

    /// Generates a `RenderNode::rect(...)` expression.
    ///
    /// Attrs: `x`, `y`, `w`, `h` (numbers or `full`), `fill`, `stroke`, `stroke_width`, `radius`, `shadow_x`, `shadow_y`, `shadow_blur`, `shadow_color`, `gradient` (linear/radial), `from`, `to`, `mid`, `mid_pos`, `x1`, `y1`, `x2`, `y2` (linear points), `cx`, `cy`, `r` (radial).
    fn emit_canvas_rect(&self, el: &Element) -> String {
        let x = self.canvas_dim("x", &el.attributes);
        let y = self.canvas_dim("y", &el.attributes);
        let w = self.canvas_dim("w", &el.attributes);
        let h = self.canvas_dim("h", &el.attributes);

        let radius = el
            .attributes
            .iter()
            .find(|a| a.key == "radius")
            .and_then(|a| a.value.parse::<f32>().ok())
            .map(|r| format!("BorderRadius::all({})", format_f32(r)))
            .unwrap_or_else(|| "BorderRadius::zero()".to_string());

        let shadow = self.canvas_shadow(&el.attributes);
        let stroke = el
            .attributes
            .iter()
            .find(|a| a.key == "stroke")
            .map(|a| self.color_expr(&a.value));
        let stroke_width = el
            .attributes
            .iter()
            .find(|a| a.key == "stroke_width")
            .and_then(|a| a.value.parse::<f32>().ok())
            .unwrap_or(1.0);
        let gradient = self.canvas_gradient_paint(&el.attributes);
        let solid_fill = el
            .attributes
            .iter()
            .find(|a| a.key == "fill")
            .map(|a| self.color_expr(&a.value));

        let rect_style =
            build_rect_style(gradient, solid_fill, stroke, stroke_width, shadow, &radius);

        format!(
            "RenderNode::rect(Rect {{ x: {x}, y: {y}, width: {w}, height: {h} }}, {rect_style})"
        )
    }

    /// Builds a `Paint::Gradient(...)` expression when `gradient:linear` or `gradient:radial` is present. Color stops: `from:` / `to:` (required), optional `mid:` with `mid_pos:` (default 0.5).
    fn canvas_gradient_paint(&self, attrs: &[Attr]) -> Option<String> {
        let gradient_type = attrs.iter().find(|a| a.key == "gradient")?.value.clone();
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

        let coord = |key: &str| {
            attrs
                .iter()
                .find(|a| a.key == key)
                .and_then(|a| a.value.parse::<f32>().ok())
                .map(format_f32)
                .unwrap_or_else(|| "0.0".to_string())
        };

        match gradient_type.trim() {
            "linear" => {
                let (x1, y1, x2, y2) = (coord("x1"), coord("y1"), coord("x2"), coord("y2"));
                Some(format!(
                    "Paint::Gradient(Gradient::linear(Point::new({x1}, {y1}), Point::new({x2}, {y2}), {stops}))"
                ))
            }
            "radial" => {
                let (cx, cy, r) = (coord("cx"), coord("cy"), coord("r"));
                Some(format!(
                    "Paint::Gradient(Gradient::radial(Point::new({cx}, {cy}), {r}, {stops}))"
                ))
            }
            _ => None,
        }
    }

    /// Extracts `shadow-*` attrs and produces a `Some(Shadow::new(...))` expression, or `None` when no shadow attrs are present.
    pub(super) fn canvas_shadow(&self, attrs: &[Attr]) -> Option<String> {
        if !attrs.iter().any(|a| a.key.starts_with("shadow")) {
            return None;
        }
        let sx = attrs
            .iter()
            .find(|a| a.key == "shadow_x")
            .and_then(|a| a.value.parse::<f32>().ok())
            .unwrap_or(0.0);
        let sy = attrs
            .iter()
            .find(|a| a.key == "shadow_y")
            .and_then(|a| a.value.parse::<f32>().ok())
            .unwrap_or(4.0);
        let blur = attrs
            .iter()
            .find(|a| a.key == "shadow_blur")
            .and_then(|a| a.value.parse::<f32>().ok())
            .unwrap_or(8.0);
        let color = attrs
            .iter()
            .find(|a| a.key == "shadow_color")
            .map(|a| self.color_expr(&a.value))
            .unwrap_or_else(|| "Color::rgba(0.0, 0.0, 0.0, 0.25)".to_string());
        Some(format!(
            "Some(Shadow::new({}, {}, {}, {}))",
            format_f32(sx),
            format_f32(sy),
            format_f32(blur),
            color
        ))
    }

    /// Generates a `Line::new(...).view()` expression.
    ///
    /// Attrs: `x1`, `y1`, `x2`, `y2` (coordinates), `color`, `stroke_width`.
    fn emit_canvas_line(&self, el: &Element) -> String {
        let coord = |key: &str| -> String {
            el.attributes
                .iter()
                .find(|a| a.key == key)
                .and_then(|a| a.value.parse::<f32>().ok())
                .map(format_f32)
                .unwrap_or_else(|| "0.0".to_string())
        };
        let x1 = coord("x1");
        let y1 = coord("y1");
        let x2 = coord("x2");
        let y2 = coord("y2");
        let color = el
            .attributes
            .iter()
            .find(|a| a.key == "color")
            .map(|a| self.color_expr(&a.value))
            .unwrap_or_else(|| "Color::BLACK".to_string());
        let width = el
            .attributes
            .iter()
            .find(|a| a.key == "stroke_width")
            .and_then(|a| a.value.parse::<f32>().ok())
            .map(format_f32)
            .unwrap_or_else(|| "1.0".to_string());
        format!(
            "Line::new(move || Point::new({x1}, {y1}), move || Point::new({x2}, {y2}), move || Stroke::new({color}, {width})).view()"
        )
    }

    /// Generates a `RenderNode::layer(opacity, blur, [...])` expression.
    ///
    /// Attrs: `opacity` (default 1.0), `blur` (default 0.0). Children are recursively emitted as canvas render-node expressions.
    fn emit_canvas_layer(&self, el: &Element) -> String {
        let opacity = el
            .attributes
            .iter()
            .find(|a| a.key == "opacity")
            .and_then(|a| a.value.parse::<f32>().ok())
            .map(format_f32)
            .unwrap_or_else(|| "1.0".to_string());
        let blur = el
            .attributes
            .iter()
            .find(|a| a.key == "blur")
            .and_then(|a| a.value.parse::<f32>().ok())
            .map(format_f32)
            .unwrap_or_else(|| "0.0".to_string());
        let child_els: Vec<&Element> = el
            .children
            .iter()
            .filter_map(|n| {
                if let ViewNode::Element(e) = n {
                    Some(e)
                } else {
                    None
                }
            })
            .collect();
        let exprs: Vec<String> = child_els
            .iter()
            .map(|child| self.emit_render_node_expr(child))
            .collect();
        if exprs.is_empty() {
            format!("RenderNode::layer({opacity}, {blur}, [])")
        } else {
            let inner = exprs
                .iter()
                .map(|e| format!("    {e}"))
                .collect::<Vec<_>>()
                .join(",\n");
            format!("RenderNode::layer({opacity}, {blur}, [\n{inner},\n])")
        }
    }

    /// Generates a `RenderNode::text(...)` expression for a `text` element inside a canvas. Uses absolute coordinates unlike layout-mode `text`.
    fn emit_canvas_text(&self, el: &Element) -> String {
        let content = el.content.as_deref().unwrap_or("");
        let x = self.canvas_dim("x", &el.attributes);
        let y = self.canvas_dim("y", &el.attributes);
        let w = self.canvas_dim("w", &el.attributes);
        let h = self.canvas_dim("h", &el.attributes);

        let size = el
            .attributes
            .iter()
            .find(|a| a.key == "size")
            .and_then(|a| a.value.parse::<f32>().ok())
            .map(format_f32)
            .unwrap_or_else(|| "14.0".to_string());

        let color = el
            .attributes
            .iter()
            .find(|a| a.key == "color")
            .map(|a| self.color_expr(&a.value))
            .unwrap_or_else(|| "Color::BLACK".to_string());

        format!(
            "RenderNode::text({}, Rect {{ x: {x}, y: {y}, width: {w}, height: {h} }}, TextStyle::new({size}, {color}))",
            rust_str(content)
        )
    }

    /// Resolves a canvas dimension attribute (`x`, `y`, `w`, `h`). `"full"` maps to `__w` (width axis) or `__h` (height axis). Omitted `w`/`h` default to `__w`/`__h`; omitted `x`/`y` default to `0.0`.
    fn canvas_dim(&self, key: &str, attrs: &[Attr]) -> String {
        let default = if key == "w" {
            "__w"
        } else if key == "h" {
            "__h"
        } else {
            "0.0"
        };
        attrs
            .iter()
            .find(|a| a.key == key)
            .map(|a| {
                let v = a.value.trim();
                if v == "full" {
                    (if key == "w" { "__w" } else { "__h" }).to_string()
                } else {
                    format_f32(v.parse::<f32>().unwrap_or(0.0))
                }
            })
            .unwrap_or_else(|| default.to_string())
    }
}

/// Recursively collects the raw values of colour-bearing attrs (see `color_attr_keys`) anywhere in a canvas child subtree, including nested `layer` children, so `emit_canvas` can clone every `$ident` signal referenced by a descendant's `fill`/`stroke`/`color`/`shadow_color`/... once, before the shared draw closure captures it.
fn collect_canvas_colors<'e>(el: &'e Element, out: &mut Vec<&'e str>) {
    for a in &el.attributes {
        if crate::registry::color_attr_keys().contains(&a.key.as_str()) {
            out.push(a.value.as_str());
        }
    }
    for child in &el.children {
        if let ViewNode::Element(e) = child {
            collect_canvas_colors(e, out);
        }
    }
}
