//! Container emitters: `col`/`row`/`grid` and `box`, plus the box gradient-paint builder.

use std::collections::HashMap;
use std::fmt::Write;

use rsx_parser::{Attr, Element};

use crate::style::format_f32;

use super::signals::{
    build_gradient_stops, closure_marker, emit_transition_prelude, has_paint, normalize_closure,
    substitute_handles, substitute_reads, wrap_signal_clones,
};
use super::{ChildEmit, ViewGen, forces_child_vec};

impl ViewGen<'_> {
    pub(super) fn emit_container(&mut self, el: &Element) -> ChildEmit {
        let var = self.next_variable_name(&el.tag);
        let pad = self.indent_str();
        let style = self.make_layout_style(&el.tag, &el.classes, &el.attributes);
        let on_press = self.on_press_call(el);

        // A `col`/`row` with paint (inline or from its class) or a `hover(...)` override upgrades to a StyledContainer so it can carry a background like `box`; otherwise it stays a plain Container.
        let pattrs = self.paint_attrs(el);
        let hover_call = self.hover_style_call(el, &pattrs);
        let transform_call = self.transform_call(el);
        let (specs, errors) = self.parse_transitions(el);
        let transitions: HashMap<String, String> = specs.into_iter().collect();
        let mut hoists: Vec<String> = Vec::new();
        // A transform also upgrades a plain col/row to a StyledContainer (only it carries `with_transform`).
        let pieces = if has_paint(&pattrs) || !hover_call.is_empty() || !transform_call.is_empty() {
            Some(self.rect_style_pieces(&pattrs, &transitions, &mut hoists))
        } else {
            None
        };

        let has_dynamic = el.children.iter().any(forces_child_vec);

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
                    "{inner_pad}StyledContainer::new(ctx, {style}, {closure}, {children})?{opacity_call}{hover_call}{on_press}{transform_call}"
                );
            }
            None => {
                let _ = writeln!(
                    code,
                    "{inner_pad}Container::new(ctx, {style}, {children})?{on_press}"
                );
            }
        }

        let _ = write!(code, "{pad}}};");
        ChildEmit::Simple { name: var, code }
    }

    pub(super) fn emit_box(&mut self, el: &Element) -> ChildEmit {
        let var = self.next_variable_name("box");
        let pad = self.indent_str();
        let layout_style = self.make_layout_style("box", &el.classes, &el.attributes);
        let on_press = self.on_press_call(el);

        // Paint merges inline attrs with the element's class (inline wins), so a `@card` class can carry fill/stroke/radius/etc. — not only inline `box` attributes. `box` is always styled.
        let pattrs = self.paint_attrs(el);
        let hover_call = self.hover_style_call(el, &pattrs);
        let transform_call = self.transform_call(el);
        let (specs, errors) = self.parse_transitions(el);
        let transitions: HashMap<String, String> = specs.into_iter().collect();
        let mut hoists: Vec<String> = Vec::new();
        let (closure, opacity_call) = self.rect_style_pieces(&pattrs, &transitions, &mut hoists);

        let has_dynamic = el.children.iter().any(forces_child_vec);

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
            "{inner_pad}StyledContainer::new(ctx, {layout_style}, {closure}, {children})?{opacity_call}{hover_call}{on_press}{transform_call}"
        );

        let _ = write!(code, "{pad}}};");
        ChildEmit::Simple { name: var, code }
    }

    /// Builds the trailing `.on_hover_style(...)` from a `hover(...)` attribute, or an empty string when
    /// there is none. The parenthesized value is a mini list of paint props (`hover(fill:x stroke:y)`);
    /// they override the element's base paint for the hovered state, so `view()` swaps to them while the
    /// mouse is over the box. Reuses `rect_style_pieces`, so `$signal` colors are cloned into the closure
    /// just like the base style. Transitions are intentionally not applied to the hover variant.
    fn hover_style_call(&mut self, el: &Element, base_pattrs: &[Attr]) -> String {
        let Some(attr) = el.attributes.iter().find(|a| a.key == "hover") else {
            return String::new();
        };
        // Overrides first so `rect_style_pieces`' first-match `find` picks the hover value over the base.
        let mut merged = parse_inline_paint_attrs(&attr.value);
        merged.extend(base_pattrs.iter().cloned());
        let mut hoists: Vec<String> = Vec::new();
        let (closure, _opacity) = self.rect_style_pieces(&merged, &HashMap::new(), &mut hoists);
        format!(".on_hover_style({closure})")
    }

    /// Builds the trailing `.on_press(...)` for a container element, or an empty string when there is no
    /// `on_press` attribute. Mirrors the button emitter: `$name` signals are cloned into the closure,
    /// `$handle` reads are rewritten to the bare handle, and a `$`-free closure keeps its source span.
    fn on_press_call(&self, el: &Element) -> String {
        let Some(attr) = el.attributes.iter().find(|a| a.key == "on_press") else {
            return String::new();
        };
        let closure = substitute_handles(&normalize_closure(&attr.value));
        // A `$` substitution breaks the byte-for-byte span, so only a `$`-free closure carries a marker.
        let marker = if attr.value.contains('$') {
            String::new()
        } else {
            closure_marker(Some(attr))
        };
        let call = wrap_signal_clones(&[attr.value.as_str()], format!("move {marker}{closure}"));
        format!(".on_press({call})")
    }

    /// Builds the trailing `.with_transform(...)` from a box's declarative transform attributes (`rotate`
    /// in degrees, `scale`/`scale_x`/`scale_y`, `translate_x`/`translate_y`), or an empty string when there
    /// are none. `scale` sets both axes unless an axis-specific value overrides it. Values may be `$signal`
    /// reads (a `rotate:$angle` animates), so they are substituted and their signals cloned into the closure;
    /// every value is cast to `f32` so integer and float literals both type-check.
    fn transform_call(&self, el: &Element) -> String {
        const KEYS: [&str; 6] = [
            "rotate",
            "scale",
            "scale_x",
            "scale_y",
            "translate_x",
            "translate_y",
        ];
        if !el.attributes.iter().any(|a| KEYS.contains(&a.key.as_str())) {
            return String::new();
        }
        let raw = |key: &str| {
            el.attributes
                .iter()
                .find(|a| a.key == key)
                .map(|a| a.value.trim().to_string())
        };
        let scale = raw("scale");
        let rotate = raw("rotate").unwrap_or_else(|| "0".into());
        let scale_x = raw("scale_x")
            .or_else(|| scale.clone())
            .unwrap_or_else(|| "1".into());
        let scale_y = raw("scale_y").or(scale).unwrap_or_else(|| "1".into());
        let tx = raw("translate_x").unwrap_or_else(|| "0".into());
        let ty = raw("translate_y").unwrap_or_else(|| "0".into());
        let values = [rotate, scale_x, scale_y, tx, ty];
        let args = values
            .iter()
            .map(|v| format!("({}) as f32", substitute_reads(v)))
            .collect::<Vec<_>>()
            .join(", ");
        let refs: Vec<&str> = values.iter().map(String::as_str).collect();
        let call = wrap_signal_clones(
            &refs,
            format!("move |__r: Rect| box_transform(__r, {args})"),
        );
        format!(".with_transform({call})")
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

/// Parses a `hover(...)` inner value — a whitespace-separated list of `key:value` paint props — into
/// `Attr`s. Paint values carry no spaces (color tokens, `#hex`, numbers), so a simple split suffices.
/// A token without a `:` (a bare flag) is ignored: hover overrides are always keyed paint props.
fn parse_inline_paint_attrs(value: &str) -> Vec<Attr> {
    value
        .split_whitespace()
        .filter_map(|tok| {
            let (key, val) = tok.split_once(':')?;
            Some(Attr {
                key: key.to_string(),
                value: val.to_string(),
                is_quoted: false,
                value_start: 0,
            })
        })
        .collect()
}
