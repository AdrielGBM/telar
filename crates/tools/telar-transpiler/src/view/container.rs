//! Container emitters: `col`/`row`/`grid` and `box`, plus the box gradient-paint builder.

use std::collections::HashMap;
use std::fmt::Write;

use telar_parser::{Attr, Element, Value};

use crate::naming::to_pascal_case;
use crate::style::format_f32;

use super::signals::{
    closure_marker, emit_transition_prelude, has_paint, normalize_closure, substitute_handles,
    substitute_reads, wrap_signal_clones,
};
use super::{ChildEmit, ChildMode, ViewGen, expr_marker, forces_child_vec};

impl ViewGen<'_> {
    pub(super) fn emit_container(&mut self, el: &Element) -> ChildEmit {
        self.emit_styled_container(el, false)
    }

    pub(super) fn emit_box(&mut self, el: &Element) -> ChildEmit {
        self.emit_styled_container(el, true)
    }

    /// Emits a `col`/`row`/`grid` or a `box` Container, collecting children and wiring declarative paint, transform, hover and event closures. `box` (`always_style`) is always a StyledContainer so it can carry a background; a `col`/`row` only upgrades from a plain Container when it carries paint (inline or class-borne) or one of the styling attrs below.
    fn emit_styled_container(&mut self, el: &Element, always_style: bool) -> ChildEmit {
        let var = self.next_variable_name(&el.tag);
        let pad = self.indent_str();
        let style = self.make_layout_style(&el.tag, &el.classes, &el.attributes);
        // A computed layout prop makes the whole style reactive: `LayoutStyle` is a value handed to the tree once, not a closure the renderer re-runs, so the node keeps an effect that re-resolves it.
        let reactive = self.reactive_layout_values(&el.tag, &el.attributes);
        let styled_by = if reactive.is_empty() {
            String::new()
        } else {
            let raw: Vec<&str> = reactive.iter().map(String::as_str).collect();
            format!(
                ".styled_by({})",
                self.clone_captures(&raw, format!("move || {style}"))
            )
        };
        // `cursor:pointer` names a variant; anything else is the expression it is, and a reactive value becomes a reading so the shape follows it the way a colour does.
        let cursor = el
            .attributes
            .iter()
            .find(|a| a.key == "cursor")
            .map(|a| {
                let raw = a.value.text();
                let value = super::redundant_parens(raw.trim()).unwrap_or(raw.trim());
                match crate::registry::cursor_keyword(value) {
                    Some(variant) => format!(".cursor({variant})"),
                    None if value.contains('$') => format!(
                        ".cursor({})",
                        wrap_signal_clones(
                            &[value],
                            format!("Reactive::of(move || {})", substitute_reads(value))
                        )
                    ),
                    None => format!(".cursor({})", value),
                }
            })
            .unwrap_or_default();
        // Commas are separators too, for the `drag_button:secondary,auxiliary` spelling predating the paren form.
        let drag_button = el
            .attributes
            .iter()
            .find(|a| a.key == "drag_button")
            .map(|a| {
                a.value
                    .text()
                    .split([',', ' '])
                    .filter(|b| !b.trim().is_empty())
                    .map(|b| format!(".drag_button(PointerButton::{})", to_pascal_case(b.trim())))
                    .collect::<String>()
            })
            .unwrap_or_default();
        let drag_threshold = el
            .attributes
            .iter()
            .find(|a| a.key == "drag_threshold")
            .map(|a| a.value.text().trim().to_string())
            .filter(|value| !value.is_empty())
            .map(|value| {
                format!(
                    ".drag_threshold({})",
                    crate::style::number_or(&value, "0.0")
                )
            })
            .unwrap_or_default();
        // Emitted on a plain `Container` too, so calling a `col` the navigation does not turn it into a styled box.
        let role = el
            .attributes
            .iter()
            .find(|a| a.key == "role")
            .and_then(|a| crate::registry::role_variant(a.value.text().trim()))
            .map(|variant| format!(".role(::telar::Role::{variant})"))
            .unwrap_or_default();
        let click_through = el
            .attributes
            .iter()
            .find(|a| a.key == "click_through")
            .map(|_| ".click_through(true)".to_string())
            .unwrap_or_default();
        let holds_stroke = el
            .attributes
            .iter()
            .find(|a| a.key == "holds_stroke")
            .map(|_| ".holds_stroke()".to_string())
            .unwrap_or_default();
        let on_press = self.on_press_call(el);
        // A forwarded (non-closure) `on_press` wires `.maybe_on_press`, which only StyledContainer has.
        let on_press_forwarded = el
            .attributes
            .iter()
            .find(|a| a.key == "on_press")
            .is_some_and(|a| !a.value.is_closure());

        let attrs = self.effective_attrs(el);
        let hover_call = self.state_style_call(el, "hover_style", "hover_style", &attrs);
        let active_call = self.state_style_call(el, "active_style", "active_style", &attrs);
        let disabled_call = self.state_style_call(el, "disabled_style", "disabled_style", &attrs);
        let disabled = self.disabled_call(el);
        let focus_ring = self.state_style_call(el, "focus_style", "focus_style", &[]);
        let on_hover = self.closure_attr_call(el, "on_hover", "on_hover");
        let on_pointer_move = self.closure_attr_call(el, "on_pointer_move", "on_pointer_move");
        let on_key = self.closure_attr_call(el, "on_key", "on_key");
        let on_drag = self.closure_attr_call(el, "on_drag", "on_drag");
        let on_drag_end = self.closure_attr_call(el, "on_drag_end", "on_drag_end");
        let on_scroll = self.closure_attr_call(el, "on_scroll", "on_scroll");
        let on_focus = self.closure_attr_call(el, "on_focus", "on_focus");
        let on_long_press = self.closure_attr_call(el, "on_long_press", "on_long_press");
        let on_alt_press = self.closure_attr_call(el, "on_alt_press", "on_alt_press");
        let (specs, errors) = self.parse_transitions(el);
        let transitions: HashMap<String, String> = specs.into_iter().collect();
        let mut hoists: Vec<String> = Vec::new();
        let transform_call = self.transform_call(el, &transitions, &mut hoists);
        // What the container says about the text below it: `col font_size:11` draws no text, it names the size everything under it starts from.
        let declaring = self.declaring_call(&attrs, &transitions, &mut hoists);

        // Any of these forces the StyledContainer upgrade. `on_press` is excluded because its closure form wires on a plain Container; `on_press_forwarded` covers the other case.
        let styling = format!(
            "{hover_call}{active_call}{disabled_call}{focus_ring}{disabled}{transform_call}{on_hover}{on_pointer_move}{on_key}{on_drag}{on_drag_end}{on_scroll}{on_focus}{on_long_press}{on_alt_press}{cursor}{drag_button}{drag_threshold}{click_through}"
        );
        let pieces =
            if always_style || has_paint(&attrs) || !styling.is_empty() || on_press_forwarded {
                Some(self.rect_style_pieces(&attrs, &transitions, &mut hoists))
            } else {
                None
            };

        let mode = Self::child_mode(&el.children);

        let is_row = self.container_is_row(&el.tag, &el.classes, &el.attributes);
        self.indent += 1;
        let inner_pad = self.indent_str();
        let child_emits: Vec<ChildEmit> = self.within_host(is_row, |g| {
            g.with_child_sink(mode, |g| {
                el.children.iter().map(|child| g.emit_node(child)).collect()
            })
        });
        self.indent -= 1;

        let mut code = String::new();
        let _ = writeln!(code, "{pad}let {var} = {{");

        let children =
            self.emit_children_collection(&mut code, &child_emits, &inner_pad, mode, &[]);
        // Routing through `from_slots` lets the children interleave in this node and inherit its flex direction.
        let ctor = if mode == ChildMode::Slots {
            "from_slots"
        } else {
            "new"
        };
        emit_transition_prelude(&mut code, &inner_pad, &errors, &hoists);
        // `track_rect:$sig` needs the node, which exists only once the widget is built, so the effect is chained onto it with `keeping` rather than anything longer-lived.
        let track = self.track_rect_tail(el, &inner_pad);
        let bind = if track.is_empty() {
            ""
        } else {
            "let __tracked = "
        };
        let terminator = if track.is_empty() { "" } else { ";" };
        match pieces {
            Some((closure, opacity_call)) => {
                let _ = writeln!(
                    code,
                    "{inner_pad}{bind}StyledContainer::{ctor}({style}, {closure}, {children})?{opacity_call}{hover_call}{active_call}{disabled_call}{focus_ring}{disabled}{on_press}{transform_call}{on_hover}{on_pointer_move}{on_key}{on_drag}{on_drag_end}{on_scroll}{on_focus}{on_long_press}{on_alt_press}{cursor}{drag_button}{drag_threshold}{click_through}{holds_stroke}{role}{styled_by}{declaring}{terminator}"
                );
            }
            None => {
                let _ = writeln!(
                    code,
                    "{inner_pad}{bind}Container::{ctor}({style}, {children})?{on_press}{role}{styled_by}{declaring}{terminator}"
                );
            }
        }
        code.push_str(&track);

        let _ = write!(code, "{pad}}};");
        ChildEmit::Simple { name: var, code }
    }

    /// Emits an `overlay` as an `Overlay` widget: a top-layer, out-of-flow portal (see `ui_core::Overlay`). Children are collected like a container; layout attrs (`align`/`justify`/`pad`) position the content within the viewport-filling layer.
    pub(super) fn emit_overlay(&mut self, el: &Element) -> ChildEmit {
        let var = self.next_variable_name("overlay");
        let pad = self.indent_str();
        let style = self.make_layout_style("overlay", &el.classes, &el.attributes);

        // `Overlay::new` takes a plain child vec, so a reactive region here stays a boxed `ReactiveList`.
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

    /// Builds one trailing `.on_X_style(...)` from an `X_style(...)` attribute — the paint swap for a state, written as a mini list of paint props (`hover_style(fill:x stroke:y)`). Reuses `rect_style_pieces`, so a `$signal` colour is cloned into the closure just like the base style. Transitions are deliberately not applied to a state variant.
    ///
    /// `base` is prepended to the overrides so `rect_style_pieces`. first-match `find` picks the state value over the base one. The focus ring passes an empty `base`: what the ring does not name comes from whichever state won underneath it at paint time, not from the base style at build time.
    fn state_style_call(&mut self, el: &Element, key: &str, method: &str, base: &[Attr]) -> String {
        let Some(attr) = el.attributes.iter().find(|a| a.key == key) else {
            return String::new();
        };
        let mut merged = parse_inline_paint_attrs(attr.value.text());
        merged.extend(base.iter().cloned());
        let mut hoists: Vec<String> = Vec::new();
        let (closure, _opacity) = self.rect_style_pieces(&merged, &HashMap::new(), &mut hoists);
        format!(".{method}({closure})")
    }

    /// Builds the trailing `.disabled(...)` from a `disabled:` attribute.
    ///
    /// A closure rather than a value, so a `$signal` is re-read instead of frozen at construction — the same treatment a reactive colour or string prop gets, and deliberately *not* the layout path: `width:$sig` re-runs the whole `LayoutStyle`, and whether a control is usable is not a layout property.
    fn disabled_call(&self, el: &Element) -> String {
        let Some(attr) = el.attributes.iter().find(|a| a.key == "disabled") else {
            return String::new();
        };
        let value = attr.value.text().trim();
        if attr.value.is_flag() {
            return ".disabled(|| true)".to_string();
        }
        let read = substitute_reads(value);
        format!(
            ".disabled({})",
            wrap_signal_clones(&[value], format!("move || {read}"))
        )
    }

    /// Builds a trailing `.{method}(...)` from a closure-valued attribute (`on_press`/`on_hover`/`on_key`/ `on_drag`/`on_focus`/`on_long_press`), or an empty string when the attribute is absent. `$name` signals are cloned into the closure, `$handle` reads are rewritten to the bare handle, and a `$`-free closure keeps its source span.
    fn closure_attr_call(&self, el: &Element, key: &str, method: &str) -> String {
        let Some(attr) = el.attributes.iter().find(|a| a.key == key) else {
            return String::new();
        };
        // A non-closure value is an `Option<handler>` being forwarded, and the `maybe_` form leaves the box untouched on `None`. A no-op stand-in would report the event handled and swallow the click.
        if !attr.value.is_closure() {
            let text = attr.value.text().trim();
            let (offset, inner) = match super::redundant_parens(text) {
                Some(inner) => (1, inner),
                None => (0, text),
            };
            let marker = expr_marker(attr.value_start + offset, inner.len());
            return format!(".maybe_{method}({marker}{inner})");
        }
        format!(".{method}({})", self.emit_closure_value(attr))
    }

    /// Desugars a closure-valued attribute into a `move` closure: `$name` signals are cloned in, `$handle` reads are rewritten to the bare handle, and a `$`-free closure keeps its source span (so LSP completion works inside it). Shared by `closure_attr_call` (element event attrs) and the component closure-prop arm, which wrap the result as `.method(..)` and `Box::new(..)` respectively.
    pub(super) fn emit_closure_value(&self, attr: &Attr) -> String {
        let closure = substitute_handles(&normalize_closure(attr.value.text()));
        // A `$` substitution breaks the byte-for-byte span, so only a `$`-free closure carries a marker.
        let marker = if attr.value.text().contains('$') {
            String::new()
        } else {
            closure_marker(Some(attr))
        };
        wrap_signal_clones(&[attr.value.text()], format!("move {marker}{closure}"))
    }

    fn on_press_call(&self, el: &Element) -> String {
        self.closure_attr_call(el, "on_press", "on_press")
    }

    /// Builds the trailing `.with_transform(...)` from a box's declarative transform attributes (`rotate` in degrees, `scale`/`scale_x`/`scale_y`, `translate_x`/`translate_y`), or an empty string when there are none. `scale` sets both axes unless an axis-specific value overrides it. Values may be `$signal` reads (a `rotate:$angle` animates), so they are substituted and their signals cloned into the closure; every value is cast to `f32` so integer and float literals both type-check. `track_rect:$sig` — mirror this element's laid-out rect into `sig`, so a sibling can be positioned or painted from where this one ended up.
    ///
    /// `track_layout` hands back the node's own rect signal; this copies it into the author's signal, which is what makes the value reachable from the rest of their `[view]` and `[logic]`. The mirroring effect is kept on the widget, so it stops when the widget goes rather than firing at a node that is gone.
    ///
    /// The copy is guarded, which is the difference between a rect somebody can build on and one nothing can afford to read: a signal notifies on every write, so an unguarded mirror wakes each of its readers on every frame the layout runs, whether or not the rectangle moved.
    fn track_rect_tail(&self, el: &Element, pad: &str) -> String {
        let Some(attr) = el.attributes.iter().find(|a| a.key == "track_rect") else {
            return String::new();
        };
        let target = attr.value.text().trim().trim_start_matches('$');
        if target.is_empty() {
            return String::new();
        }
        format!(
            "{pad}let __rect = track_layout(__tracked.layout_node()).expect(\"a container registers its rect\");\n\
             {pad}let {target} = {target}.clone();\n\
             {pad}effect(move || {{\n\
             {pad}    let __now = __rect.get();\n\
             {pad}    // Guarded, and not as an optimisation: a signal notifies on every write, and the layout\n\
             {pad}    // runs every frame something moves — so an unguarded copy would wake every reader of\n\
             {pad}    // this rect sixty times a second for a rectangle that did not change.\n\
             {pad}    if {target}.peek() != __now {{\n\
             {pad}        {target}.set(__now);\n\
             {pad}    }}\n\
             {pad}}});\n\
             {pad}__tracked\n"
        )
    }

    fn transform_call(
        &mut self,
        el: &Element,
        transitions: &HashMap<String, String>,
        hoists: &mut Vec<String>,
    ) -> String {
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
                .map(|a| a.value.text().trim().to_string())
        };
        let scale = raw("scale");
        let rotate = raw("rotate").unwrap_or_else(|| "0".into());
        let scale_x = raw("scale_x")
            .or_else(|| scale.clone())
            .unwrap_or_else(|| "1".into());
        let scale_y = raw("scale_y").or(scale).unwrap_or_else(|| "1".into());
        let tx = raw("translate_x").unwrap_or_else(|| "0".into());
        let ty = raw("translate_y").unwrap_or_else(|| "0".into());
        // `scale` stands in for both axes when neither was given its own value, matching how the value resolves.
        let axis_prop = |own: &'static str| match transitions.contains_key(own) {
            true => own,
            false => "scale",
        };
        let values = [
            (rotate, "rotate"),
            (scale_x, axis_prop("scale_x")),
            (scale_y, axis_prop("scale_y")),
            (tx, "translate_x"),
            (ty, "translate_y"),
        ];
        let mut args = Vec::new();
        for (value, prop) in &values {
            let read = format!("({}) as f32", substitute_reads(value));
            args.push(match transitions.get(*prop) {
                Some(curve) => self.wrap_transition(curve, &read, hoists),
                None => read,
            });
        }
        let args = args.join(", ");
        let refs: Vec<&str> = values.iter().map(|(v, _)| v.as_str()).collect();
        let call = wrap_signal_clones(
            &refs,
            format!("move |__r: Rect| box_transform(__r, {args})"),
        );
        format!(".with_transform({call})")
    }

    /// The `Paint::Gradient(…)` a `fill:linear(…)` or `fill:radial(…)` builds, or `None` for a fill that is a plain colour. Uses the paint closure's `r` for the absolute points a gradient needs; see [`crate::gradient`] for the value's own shape.
    pub(super) fn box_gradient_paint(&self, attrs: &[Attr]) -> Option<String> {
        let value = attrs.iter().find(|a| a.key == "fill")?.value.text().trim();
        let (kind, args) = crate::gradient::split_call(value)?;
        let parts = crate::gradient::parse(kind, args)?;
        let stops: Vec<String> = parts
            .stops
            .iter()
            .map(|(pos, color)| format!("({}, {})", format_f32(*pos), self.color_expr(color)))
            .collect();
        let stops = format!("&[{}]", stops.join(", "));
        Some(match parts.shape {
            crate::gradient::Shape::Linear(line) => {
                format!("Paint::Gradient(Gradient::linear({line}, {stops}))")
            }
            crate::gradient::Shape::Radial(radius) => format!(
                "Paint::Gradient(Gradient::radial(Point::new(r.x + r.width * 0.5, r.y + r.height * 0.5), {radius}, {stops}))"
            ),
        })
    }
}

/// Parses a `hover(...)` inner value — a whitespace-separated list of `key:value` paint props — into `Attr`s. Paint values carry no spaces (color tokens, `#hex`, numbers), so a simple split suffices. A token without a `:` (a bare flag) is ignored: hover overrides are always keyed paint props.
fn parse_inline_paint_attrs(value: &str) -> Vec<Attr> {
    value
        .split_whitespace()
        .filter_map(|tok| {
            let (key, val) = tok.split_once(':')?;
            Some(Attr {
                key: key.to_string(),
                value: Value::Expr(val.to_string()),
                value_start: 0,
            })
        })
        .collect()
}
