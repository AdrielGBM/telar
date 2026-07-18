//! Container emitters: `col`/`row`/`grid` and `box`, plus the box gradient-paint builder.

use std::collections::HashMap;
use std::fmt::Write;

use rsx_parser::{Attr, Element};

use crate::style::format_f32;

use super::signals::{
    build_gradient_stops, closure_marker, emit_transition_prelude, has_paint, normalize_closure,
    substitute_handles, substitute_reads, wrap_signal_clones,
};
use super::{ChildEmit, ChildMode, ViewGen, forces_child_vec};

impl ViewGen<'_> {
    pub(super) fn emit_container(&mut self, el: &Element) -> ChildEmit {
        self.emit_styled_container(el, false)
    }

    pub(super) fn emit_box(&mut self, el: &Element) -> ChildEmit {
        self.emit_styled_container(el, true)
    }

    /// Emits a `col`/`row`/`grid` or a `box` Container, collecting children and wiring declarative paint,
    /// transform, hover and event closures. `box` (`always_style`) is always a StyledContainer so it can
    /// carry a background; a `col`/`row` only upgrades from a plain Container when it carries paint (inline
    /// or class-borne) or one of the styling attrs below.
    fn emit_styled_container(&mut self, el: &Element, always_style: bool) -> ChildEmit {
        let var = self.next_variable_name(&el.tag);
        let pad = self.indent_str();
        let style = self.make_layout_style(&el.tag, &el.classes, &el.attributes);
        let on_press = self.on_press_call(el);

        let pattrs = self.paint_attrs(el);
        let hover_call = self.hover_style_call(el, &pattrs);
        let active_call = self.active_style_call(el, &pattrs);
        let transform_call = self.transform_call(el);
        let on_hover = self.closure_attr_call(el, "on_hover", "on_hover");
        let on_key = self.closure_attr_call(el, "on_key", "on_key");
        let on_drag = self.closure_attr_call(el, "on_drag", "on_drag");
        let on_focus = self.closure_attr_call(el, "on_focus", "on_focus");
        let on_long_press = self.closure_attr_call(el, "on_long_press", "on_long_press");
        let (specs, errors) = self.parse_transitions(el);
        let transitions: HashMap<String, String> = specs.into_iter().collect();
        let mut hoists: Vec<String> = Vec::new();

        // These trailing calls carry only on a StyledContainer, so any one of them forces the upgrade; `on_press` is excluded because it wires on a plain Container too. `box` (`always_style`) skips the check.
        let styling = format!(
            "{hover_call}{active_call}{transform_call}{on_hover}{on_key}{on_drag}{on_focus}{on_long_press}"
        );
        let pieces = if always_style || has_paint(&pattrs) || !styling.is_empty() {
            Some(self.rect_style_pieces(&pattrs, &transitions, &mut hoists))
        } else {
            None
        };

        let mode = Self::child_mode(&el.children);

        self.indent += 1;
        let inner_pad = self.indent_str();
        let child_emits: Vec<ChildEmit> = self.with_child_sink(mode, |g| {
            el.children.iter().map(|child| g.emit_node(child)).collect()
        });
        self.indent -= 1;

        let mut code = String::new();
        let _ = writeln!(code, "{pad}let {var} = {{");

        let children =
            self.emit_children_collection(&mut code, &child_emits, &inner_pad, mode, &[]);
        // A reactive fragment among the children routes them all through `from_slots`, so they interleave in
        // this container's node and inherit its flex direction (transparent `for`/`if`).
        let ctor = if mode == ChildMode::Slots {
            "from_slots"
        } else {
            "new"
        };
        emit_transition_prelude(&mut code, &inner_pad, &errors, &hoists);
        match pieces {
            Some((closure, opacity_call)) => {
                let _ = writeln!(
                    code,
                    "{inner_pad}StyledContainer::{ctor}({style}, {closure}, {children})?{opacity_call}{hover_call}{active_call}{on_press}{transform_call}{on_hover}{on_key}{on_drag}{on_focus}{on_long_press}"
                );
            }
            None => {
                let _ = writeln!(
                    code,
                    "{inner_pad}Container::{ctor}({style}, {children})?{on_press}"
                );
            }
        }

        let _ = write!(code, "{pad}}};");
        ChildEmit::Simple { name: var, code }
    }

    /// Emits an `overlay` as an `Overlay` widget: a top-layer, out-of-flow portal (see `ui_core::Overlay`).
    /// Children are collected like a container; layout attrs (`align`/`justify`/`pad`) position the content
    /// within the viewport-filling layer.
    pub(super) fn emit_overlay(&mut self, el: &Element) -> ChildEmit {
        let var = self.next_variable_name("overlay");
        let pad = self.indent_str();
        let style = self.make_layout_style("overlay", &el.classes, &el.attributes);

        // `Overlay::new` takes a plain child vec (no `from_slots`), so a reactive `for`/`if` here stays a
        // boxed `ReactiveList` rather than a transparent fragment: cap the mode at `Vec`, never `Slots`.
        let mode = if el.children.iter().any(forces_child_vec) {
            ChildMode::Vec
        } else {
            ChildMode::Literal
        };
        self.indent += 1;
        let inner_pad = self.indent_str();
        let child_emits: Vec<ChildEmit> = self.with_child_sink(mode, |g| {
            el.children.iter().map(|child| g.emit_node(child)).collect()
        });
        self.indent -= 1;

        let mut code = String::new();
        let _ = writeln!(code, "{pad}let {var} = {{");
        let children =
            self.emit_children_collection(&mut code, &child_emits, &inner_pad, mode, &[]);
        let _ = writeln!(code, "{inner_pad}Overlay::new({style}, {children})?");
        let _ = write!(code, "{pad}}};");
        ChildEmit::Simple { name: var, code }
    }

    /// Builds the trailing `.on_hover_style(...)` from a `hover_style(...)` attribute, or an empty string when
    /// there is none. The parenthesized value is a mini list of paint props (`hover(fill:x stroke:y)`);
    /// they override the element's base paint for the hovered state, so `view()` swaps to them while the
    /// mouse is over the box. Reuses `rect_style_pieces`, so `$signal` colors are cloned into the closure
    /// just like the base style. Transitions are intentionally not applied to the hover variant.
    fn hover_style_call(&mut self, el: &Element, base_pattrs: &[Attr]) -> String {
        let Some(attr) = el.attributes.iter().find(|a| a.key == "hover_style") else {
            return String::new();
        };
        // Overrides first so `rect_style_pieces`' first-match `find` picks the hover value over the base.
        let mut merged = parse_inline_paint_attrs(&attr.value);
        merged.extend(base_pattrs.iter().cloned());
        let mut hoists: Vec<String> = Vec::new();
        let (closure, _opacity) = self.rect_style_pieces(&merged, &HashMap::new(), &mut hoists);
        format!(".on_hover_style({closure})")
    }

    /// Builds the trailing `.on_active_style(...)` from an `active_style(...)` attribute — the pressed /
    /// CSS `:active` paint swap, symmetric with [`hover_style_call`](Self::hover_style_call): a
    /// whitespace-separated list of paint props (`active_style(fill:x stroke:y)`) that override the base
    /// paint while a primary pointer is held down inside the box, taking precedence over the hover style.
    fn active_style_call(&mut self, el: &Element, base_pattrs: &[Attr]) -> String {
        let Some(attr) = el.attributes.iter().find(|a| a.key == "active_style") else {
            return String::new();
        };
        let mut merged = parse_inline_paint_attrs(&attr.value);
        merged.extend(base_pattrs.iter().cloned());
        let mut hoists: Vec<String> = Vec::new();
        let (closure, _opacity) = self.rect_style_pieces(&merged, &HashMap::new(), &mut hoists);
        format!(".on_active_style({closure})")
    }

    /// Builds a trailing `.{method}(...)` from a closure-valued attribute (`on_press`/`on_hover`/`on_key`/
    /// `on_drag`/`on_focus`/`on_long_press`), or an empty string when the attribute is absent. `$name` signals are cloned into the closure,
    /// `$handle` reads are rewritten to the bare handle, and a `$`-free closure keeps its source span.
    fn closure_attr_call(&self, el: &Element, key: &str, method: &str) -> String {
        let Some(attr) = el.attributes.iter().find(|a| a.key == key) else {
            return String::new();
        };
        format!(".{method}({})", self.emit_closure_value(attr))
    }

    /// Desugars a closure-valued attribute into a `move` closure: `$name` signals are cloned in, `$handle`
    /// reads are rewritten to the bare handle, and a `$`-free closure keeps its source span (so LSP
    /// completion works inside it). Shared by `closure_attr_call` (element event attrs) and the component
    /// closure-prop arm, which wrap the result as `.method(..)` and `Box::new(..)` respectively.
    pub(super) fn emit_closure_value(&self, attr: &Attr) -> String {
        let closure = substitute_handles(&normalize_closure(&attr.value));
        // A `$` substitution breaks the byte-for-byte span, so only a `$`-free closure carries a marker.
        let marker = if attr.value.contains('$') {
            String::new()
        } else {
            closure_marker(Some(attr))
        };
        wrap_signal_clones(&[attr.value.as_str()], format!("move {marker}{closure}"))
    }

    fn on_press_call(&self, el: &Element) -> String {
        self.closure_attr_call(el, "on_press", "on_press")
    }

    /// Builds the trailing `.with_transform(...)` from a box's declarative transform attributes (`rotate`
    /// in degrees, `scale`/`scale_x`/`scale_y`, `translate_x`/`translate_y`), or an empty string when there
    /// are none. `scale` sets both axes unless an axis-specific value overrides it. Values may be `$signal`
    /// reads (a `rotate:$angle` animates), so they are substituted and their signals cloned into the closure;
    /// every value is cast to `f32` so integer and float literals both type-check.
    fn transform_call(&self, el: &Element) -> String {
        if !el
            .attributes
            .iter()
            .any(|a| crate::registry::TRANSFORM_ATTR_KEYS.contains(&a.key.as_str()))
        {
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
                // `radial_radius:N` — explicit pixel radius; default is half the shorter side.
                let radius_expr = attrs
                    .iter()
                    .find(|a| a.key == "radial_radius")
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
                i18n: false,
                value_start: 0,
            })
        })
        .collect()
}
