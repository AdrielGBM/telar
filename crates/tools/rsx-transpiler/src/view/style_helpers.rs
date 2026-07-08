//! Paint/layout style helpers shared across the styled emitters: paint-attr merging, `RectStyle` assembly, opacity/transition closures, and `LayoutStyle` building.

use std::collections::HashMap;

use rsx_parser::{Attr, Element};

use crate::naming::style_function_name;
use crate::style::{format_f32, layout_prop_call};

use super::ViewGen;
use super::signals::{
    build_rect_style, captured_idents, is_paint_key, substitute_reads, wrap_signal_clones,
};

impl ViewGen<'_> {
    /// The effective paint attributes for an element: its inline attrs followed by the paint props of its
    /// classes. Inline wins (the paint helpers take the first `.find()` match, and inline attrs come first);
    /// among multiple classes a later one overrides an earlier one, so classes are appended in REVERSE order
    /// (the last class's props land ahead of the first's and win the `.find()`).
    pub(super) fn paint_attrs(&self, el: &Element) -> Vec<Attr> {
        let mut attrs = el.attributes.clone();
        for name in el.classes.iter().rev() {
            if let Some(class) = self.classes.iter().find(|c| &c.name == name) {
                for prop in &class.props {
                    if is_paint_key(&prop.key) {
                        attrs.push(Attr {
                            key: prop.key.clone(),
                            value: prop.value.clone(),
                            is_quoted: false,
                            value_start: 0,
                        });
                    }
                }
            }
        }
        attrs
    }

    /// Builds the `(styling closure, .with_opacity(..) suffix)` for a styled container from paint attributes. The closure's param is `r` only when a gradient needs the rendered bounds. `transitions` maps an animated property to its `motion::` curve; any `fill`/`stroke`/`opacity` it names is wrapped in the animation retarget+get block and its `Animated` handle appended to `hoists`. Any `$ident` among the paint attrs (see `color_attr_keys`) is cloned into the closure (`wrap_signal_clones`) so the outer signal binding stays usable elsewhere.
    pub(super) fn rect_style_pieces(
        &mut self,
        pattrs: &[Attr],
        transitions: &HashMap<String, String>,
        hoists: &mut Vec<String>,
    ) -> (String, String) {
        let shadow = self.canvas_shadow(pattrs);
        let gradient = self.box_gradient_paint(pattrs);
        let mut solid_fill = pattrs
            .iter()
            .find(|a| a.key == "fill")
            .map(|a| self.color_expr(&a.value));
        let mut stroke = pattrs
            .iter()
            .find(|a| a.key == "stroke")
            .map(|a| self.color_expr(&a.value));
        if let Some(curve) = transitions.get("fill")
            && let Some(fill) = solid_fill.take()
        {
            solid_fill = Some(self.wrap_transition(curve, &fill, hoists));
        }
        if let Some(curve) = transitions.get("stroke")
            && let Some(s) = stroke.take()
        {
            stroke = Some(self.wrap_transition(curve, &s, hoists));
        }
        let stroke_width = pattrs
            .iter()
            .find(|a| a.key == "stroke_width")
            .and_then(|a| a.value.parse::<f32>().ok())
            .unwrap_or(1.0);
        let radius = pattrs
            .iter()
            .find(|a| a.key == "radius")
            .and_then(|a| a.value.parse::<f32>().ok())
            .map(|r| format!("BorderRadius::all({})", format_f32(r)))
            .unwrap_or_else(|| "BorderRadius::zero()".to_string());
        let param = if gradient.is_some() { "r" } else { "_" };
        let rect_style =
            build_rect_style(gradient, solid_fill, stroke, stroke_width, shadow, &radius);
        let opacity_call = match pattrs.iter().find(|a| a.key == "opacity") {
            Some(a) => format!(
                ".with_opacity({})",
                self.opacity_closure(a, transitions, hoists)
            ),
            None => String::new(),
        };
        let raw_colors: Vec<&str> = pattrs
            .iter()
            .filter(|a| crate::registry::color_attr_keys().contains(&a.key.as_str()))
            .map(|a| a.value.as_str())
            .collect();
        let closure = wrap_signal_clones(&raw_colors, format!("move |{param}| {rect_style}"));
        (closure, opacity_call)
    }

    /// Resolves the `.with_opacity(..)` closure argument for a `StyledContainer`. Opacity is now a closure (T-3.1) so it re-reads reactively: a `$signal` becomes `move || sig.get()` (cloning captured signals), a bare number stays a static `|| 0.5`, and a `transition:opacity` wraps the value in the animation retarget+get block backed by a hoisted `Animated`.
    fn opacity_closure(
        &mut self,
        attr: &Attr,
        transitions: &HashMap<String, String>,
        hoists: &mut Vec<String>,
    ) -> String {
        let value = attr.value.trim();
        let is_reactive = value.contains('$');
        let is_static = !is_reactive && value.parse::<f32>().is_ok();
        let expr = if is_reactive {
            substitute_reads(value)
        } else if is_static {
            format_f32(value.parse::<f32>().unwrap())
        } else {
            value.to_string()
        };
        // Signals (and any in-scope loop variables) read by the closure are cloned into it so it owns 'static handles, independent of any sibling closure on the same widget. Deduped and loop-var-aware via the shared `captured_idents`; empty for a static/number value, whose branches emit no `move` closure.
        let clone_prefix: String = captured_idents(&[value], &self.loop_variables)
            .iter()
            .map(|s| format!("let {s} = {s}.clone(); "))
            .collect();
        if let Some(curve) = transitions.get("opacity") {
            let name = self.next_transition_name();
            hoists.push(format!(
                "let {name} = motion::Animated::new({expr}, {curve});"
            ));
            if is_static {
                format!("move || {{ {name}.retarget({expr}); {name}.get() }}")
            } else {
                format!("{{ {clone_prefix}move || {{ {name}.retarget({expr}); {name}.get() }} }}")
            }
        } else if is_static {
            format!("|| {expr}")
        } else {
            format!("{{ {clone_prefix}move || {expr} }}")
        }
    }

    /// Wraps a paint value expression in a `transition:` animation: hoists a persistent `Animated` seeded with the current value and returns the `{ h.retarget(value); h.get() }` block that re-targets it (a no-op when the target is unchanged) and reads the interpolated value. The `Animated` lives in the component's setup scope (built once per instance), so it persists across `view()` re-runs — the continuity requirement in F7 of the design doc.
    pub(super) fn wrap_transition(
        &mut self,
        curve: &str,
        value_expr: &str,
        hoists: &mut Vec<String>,
    ) -> String {
        let name = self.next_transition_name();
        hoists.push(format!(
            "let {name} = motion::Animated::new({value_expr}, {curve});"
        ));
        format!("{{ {name}.retarget({value_expr}); {name}.get() }}")
    }

    fn next_transition_name(&mut self) -> String {
        let name = format!("__transition_{}", self.transition_count);
        self.transition_count += 1;
        name
    }

    /// Parses every `transition:` attribute on `el` into `(property, curve)` pairs plus error messages surfaced as `compile_error!`. Errors: an unsupported/unparseable clause, or a property with no matching value attribute to animate.
    pub(super) fn parse_transitions(&self, el: &Element) -> (Vec<(String, String)>, Vec<String>) {
        let mut specs = Vec::new();
        let mut errors = Vec::new();
        let has_transition = el.attributes.iter().any(|a| a.key == "transition");
        if !has_transition {
            return (specs, errors);
        }
        // No loop-depth gate: a `for` here is a construction loop that runs once per component instance, so the `Animated` hoisted per iteration is already a distinct, persistent handle (see `emit_for`) — identity-by-key would only matter if loops ever gained reactive reconciliation.
        // A `fill`/`stroke`/`opacity` value may come from the element's class, not only an inline attribute (see `paint_attrs`); a `color` is always inline.
        let pattrs = self.paint_attrs(el);
        let has_value = |prop: &str| {
            el.attributes.iter().any(|a| a.key == prop) || pattrs.iter().any(|a| a.key == prop)
        };
        for attr in el.attributes.iter().filter(|a| a.key == "transition") {
            let (parsed, errs) = crate::transition::parse_transition_value(&attr.value);
            errors.extend(errs);
            for spec in parsed {
                if !has_value(&spec.prop) {
                    errors.push(format!(
                        "transition:{} has no matching `{}:` value on this element to animate",
                        spec.prop, spec.prop
                    ));
                    continue;
                }
                specs.push((spec.prop, spec.curve));
            }
        }
        (specs, errors)
    }

    /// Builds the `LayoutStyle` expression for a container: base style from the tag (or a class function), then inline attribute modifiers chained on.
    pub(super) fn make_layout_style(
        &self,
        tag: &str,
        classes: &[String],
        attrs: &[Attr],
    ) -> String {
        let mut expr = if let Some((first, rest)) = classes.split_first() {
            // The first class provides the base style (from its generated `style_*()` fn); further classes
            // compose on top by inlining their layout props as chained calls, so a later class overrides an
            // earlier one. A single class is byte-identical to before (the `rest` loop is empty).
            let mut base = format!("{}()", style_function_name(first));
            for name in rest {
                if let Some(class) = self.classes.iter().find(|c| &c.name == name) {
                    for prop in &class.props {
                        if let Some(call) = layout_prop_call(&prop.key, &prop.value) {
                            base.push_str(&call);
                        }
                    }
                }
            }
            // Apply the tag's flex direction only when NO class declares one, so a styled `row @card` still
            // lays out horizontally. `box` is included (like the no-class branch): a `LayoutStyle::new()`
            // class fn defaults to `display:block`, where `align`/`justify` are no-ops, so a classed `box`
            // needs `.flex_column()` to be a flex container and actually centre its children.
            if !classes.iter().any(|c| self.class_has_direction(c)) {
                match tag {
                    "row" | "grid" => base.push_str(".flex_row()"),
                    "col" | "box" | "overlay" => base.push_str(".flex_column()"),
                    _ => {}
                }
            }
            base
        } else {
            match tag {
                "row" => "LayoutStyle::new().flex_row()".to_string(),
                "col" | "box" | "overlay" => "LayoutStyle::new().flex_column()".to_string(),
                // `cols:` adds `.display_grid()`, so start neutral; fall back to flex_row when no cols are declared (legacy behaviour).
                "grid" => {
                    if attrs.iter().any(|a| a.key == "cols") {
                        "LayoutStyle::new()".to_string()
                    } else {
                        "LayoutStyle::new().flex_row()".to_string()
                    }
                }
                _ => "LayoutStyle::new()".to_string(),
            }
        };

        // Inline attributes are applied on top of the base style and take precedence.
        for attr in attrs {
            if let Some(call) = layout_prop_call(&attr.key, &attr.value) {
                expr.push_str(&call);
            }
        }
        expr
    }

    /// Whether the named class declares a flex `direction`, so the tag should not override it.
    fn class_has_direction(&self, class_name: &str) -> bool {
        self.classes
            .iter()
            .find(|c| c.name == class_name)
            .map(|c| c.props.iter().any(|p| p.key == "direction"))
            .unwrap_or(false)
    }
}
