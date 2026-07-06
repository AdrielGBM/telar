//! Component-call and `widget` reference emitters.

use std::fmt::Write;

use rsx_parser::{Attr, Element, ViewNode};

use crate::naming::{is_ident, to_pascal_case, to_snake_case};
use crate::style::{format_f32, hex_to_color_expr};

use super::signals::{normalize_closure, rust_str, substitute_handles, wrap_signal_clones};
use super::{ChildEmit, ViewGen, expr_marker};

impl ViewGen<'_> {
    /// Emits an unknown tag as a component function call. A no-attr, no-child tag generates `name(ctx)?`;
    /// attrs add a `NameProps { … }` struct literal; markup children are gathered into a `Slots` value
    /// (default + `slot:"name"` children) and passed as the trailing argument. The component's `.rsx`
    /// must declare a matching `pub struct NameProps` and/or use a `children` slot placeholder.
    pub(super) fn emit_component_call(&mut self, el: &Element, tag: &str) -> ChildEmit {
        let var = self.next_variable_name("node");
        let pad = self.indent_str();

        // A `slot:"name"` attr routes THIS element into a parent's named slot; the parent's call-site
        // grouping consumes it, so it is never passed as a prop.
        let props_attrs: Vec<&Attr> = el.attributes.iter().filter(|a| a.key != "slot").collect();
        let has_children = !el.children.is_empty();

        // Consult the callee's signature (workspace registry, else the built-in component catalogue) so the
        // call matches its arity. Copy out the scalars up front so the borrow doesn't tangle with later
        // `&mut self` calls.
        let sig = self.lookup_component_sig(tag);
        let (callee_has_slot, callee_has_props, props_default, field_count) = match &sig {
            Some(s) => (
                Some(s.has_slot),
                Some(s.has_props),
                s.props_default,
                Some(s.prop_fields.len()),
            ),
            None => (None, None, false, None),
        };
        let color_fields: &[String] = sig.as_ref().map_or(&[], |s| s.color_fields.as_slice());
        // Pass a `Slots` arg when there are markup children, or when the callee declares a slot (so a
        // childless call still matches its 3-arg signature). Unknown callee → the old "children ⇒ slots".
        let pass_slots = has_children || callee_has_slot == Some(true);

        let props_arg = self.component_props_arg(
            tag,
            &props_attrs,
            callee_has_props,
            props_default,
            field_count,
            color_fields,
        );

        // No children: flat call form. A childless slotted callee still gets `Slots::new()`.
        if !has_children {
            let mut args = String::from("ctx");
            if let Some(p) = &props_arg {
                args.push_str(", ");
                args.push_str(p);
            }
            if pass_slots {
                args.push_str(", Slots::new()");
            }
            let code = format!("{pad}let {var} = {tag}({args})?;");
            return ChildEmit::Simple { name: var, code };
        }

        // Children present: build a `Slots` value inside a block (so the temp names never collide with a
        // parent's `__children`), then pass it as the trailing argument.
        let mut code = String::new();
        let _ = writeln!(code, "{pad}let {var} = {{");
        self.indent += 1;
        let slots_expr = self.emit_slots(&el.children, &mut code);
        let inner_pad = self.indent_str();
        let mut args = String::from("ctx");
        if let Some(p) = &props_arg {
            args.push_str(", ");
            args.push_str(p);
        }
        args.push_str(", ");
        args.push_str(&slots_expr);
        let _ = writeln!(code, "{inner_pad}{tag}({args})?");
        self.indent -= 1;
        let _ = write!(code, "{pad}}};");
        ChildEmit::Simple { name: var, code }
    }

    /// Builds the `NameProps { … }` argument for a component call, or `None` when no props are needed.
    /// Emits `..Default::default()` only when the callee opts in (its `Props` derives `Default`) and the
    /// call omits some fields, so a full-field call stays literal (no `clippy::needless_update`). When no
    /// props are passed but the callee requires a `Props`, defaults them all.
    fn component_props_arg(
        &self,
        tag: &str,
        props_attrs: &[&Attr],
        callee_has_props: Option<bool>,
        props_default: bool,
        field_count: Option<usize>,
        color_fields: &[String],
    ) -> Option<String> {
        // Bare (not `crate::`) so the type resolves whether the component lives in this crate (via the
        // `use super::*` glob at crate root) or in a component library re-exported through `use rsx::*`.
        let props_type = to_pascal_case(tag) + "Props";
        if !props_attrs.is_empty() {
            let fields: Vec<String> = props_attrs
                .iter()
                .map(|attr| {
                    let value = if color_fields.iter().any(|f| f == &attr.key) {
                        self.component_color_attr_expr(attr)
                    } else {
                        self.component_attr_expr(attr)
                    };
                    format!("{}: {}", attr.key, value)
                })
                .collect();
            let omits = field_count.is_some_and(|n| props_attrs.len() < n);
            let tail = if props_default && omits {
                ", ..Default::default()"
            } else {
                ""
            };
            Some(format!("{props_type} {{ {}{tail} }}", fields.join(", ")))
        } else if callee_has_props == Some(true) {
            // No props passed but the callee has a `Props`: default them all (works when it derives Default).
            Some(format!("{props_type} {{ ..Default::default() }}"))
        } else {
            None
        }
    }

    /// Looks up a callee's signature: the workspace registry first, then the built-in component catalogue
    /// (`button`/`heading`/`section`) so a call resolves correctly even before the registry is seeded
    /// (isolated transpiles, tests). Returns an owned clone so the borrow doesn't tangle with `&mut self`.
    fn lookup_component_sig(&self, tag: &str) -> Option<crate::codegen::ComponentSig> {
        if let Some(s) = self.registry.and_then(|r| r.get(tag)) {
            return Some(s.clone());
        }
        crate::codegen::external_component_sigs()
            .into_iter()
            .find(|(name, _)| *name == tag)
            .map(|(_, sig)| sig)
    }

    /// A reactive colour prop (e.g. a button's `fill`): a `move ||` closure re-read every frame, so a
    /// theme token or `$signal` colour re-colours live. Mirrors the treatment of a `text` colour: the
    /// raw value is scanned for `$idents` to clone in, and `color_expr` applies the same `[style]`/theme
    /// precedence as built-in elements. The Props field is expected to be `Box<dyn Fn() -> Color>`.
    fn component_color_attr_expr(&self, attr: &Attr) -> String {
        let color = self.color_expr(&attr.value);
        let wrapped = wrap_signal_clones(&[attr.value.as_str()], format!("move || {color}"));
        format!("Box::new({wrapped})")
    }

    /// Emits the markup children of a component call into a `Slots` value: a child written with
    /// `slot:"name"` goes to that named slot; every other child (including `if`/`for` control flow) goes
    /// to the default slot. Returns the expression naming the built value (`__slots`).
    fn emit_slots(&mut self, children: &[ViewNode], code: &mut String) -> String {
        let pad = self.indent_str();
        let _ = writeln!(code, "{pad}let mut __slots = Slots::new();");
        let _ = writeln!(
            code,
            "{pad}let mut __children: Vec<Box<dyn LayoutItem>> = Vec::new();"
        );
        for child in children {
            let slot_name = match child {
                ViewNode::Element(el) => el
                    .attributes
                    .iter()
                    .find(|a| a.key == "slot")
                    .map(|a| a.value.clone()),
                _ => None,
            };
            // Strip the `slot` attr before emitting a named child, so a component child doesn't receive it
            // as a prop and a builtin doesn't see a stray attribute.
            let emit = match (child, &slot_name) {
                (ViewNode::Element(el), Some(_)) => {
                    let mut stripped = el.clone();
                    stripped.attributes.retain(|a| a.key != "slot");
                    self.emit_node(&ViewNode::Element(stripped))
                }
                _ => self.emit_node(child),
            };
            match emit {
                ChildEmit::Simple { name, code: c } => {
                    let _ = writeln!(code, "{c}");
                    match &slot_name {
                        Some(n) => {
                            let _ = writeln!(
                                code,
                                "{pad}__slots.push(Some({}), box_item({name}));",
                                rust_str(n)
                            );
                        }
                        None => {
                            let _ = writeln!(code, "{pad}__children.push(box_item({name}));");
                        }
                    }
                }
                ChildEmit::Dynamic { code: c } => {
                    let _ = writeln!(code, "{c}");
                }
            }
        }
        let _ = writeln!(
            code,
            "{pad}for __c in __children {{ __slots.push(None, __c); }}"
        );
        "__slots".to_string()
    }

    /// Emits a `children` slot placeholder: splices the caller-supplied children for this slot into the
    /// enclosing container's `__children` vec. `children` drains the default slot; `children name:"x"`
    /// drains the named slot `"x"`. Dynamic, so the container builds a `__children` vec (see `forces_child_vec`).
    pub(super) fn emit_slot(&mut self, el: &Element) -> ChildEmit {
        let pad = self.indent_str();
        let expr = match el.attributes.iter().find(|a| a.key == "name") {
            Some(a) => format!("__slots.take({})", rust_str(&a.value)),
            None => "__slots.take_default()".to_string(),
        };
        ChildEmit::Dynamic {
            code: format!("{pad}__children.extend({expr});"),
        }
    }

    /// Converts a component attribute to a Rust expression:
    /// - a quoted attr (`label:"text"`) becomes a string literal;
    /// - a bare flag (`elevated`) or `true`/`false` becomes a `bool`;
    /// - a closure (`on_tap(|| $x += 1)`) becomes a boxed `move` closure with `$signal`s cloned in;
    /// - a lone `$signal` becomes the cloned handle (`count.clone()`);
    /// - numbers become `f32`, hex/named colors resolve via `color_expr`, and anything else (an enum path
    ///   like `Variant::Primary`, a Rust expression) is forwarded verbatim.
    ///
    /// Simple lowercase identifiers (e.g. `fill:primary`) are routed through `color_expr` so they follow the same [style]-vs-theme precedence as built-in elements. PascalCase or complex expressions are passed through verbatim.
    fn component_attr_expr(&self, attr: &Attr) -> String {
        if attr.is_quoted {
            return rust_str(&attr.value);
        }
        let v = attr.value.trim();
        // Bare flag (`elevated`) -> `true`; explicit `true`/`false` pass through as bools.
        if v.is_empty() {
            return "true".to_string();
        }
        if v == "true" || v == "false" {
            return v.to_string();
        }
        // Closure prop: box a `move` closure, cloning captured `$signal`s and rewriting `$handle` reads —
        // the same treatment `on_press` gets. The Props field is expected to be a `Box<dyn Fn(..)>`.
        if v.starts_with('|') {
            let closure = substitute_handles(&normalize_closure(&attr.value));
            let wrapped = wrap_signal_clones(&[attr.value.as_str()], format!("move {closure}"));
            return format!("Box::new({wrapped})");
        }
        // A lone `$signal`: pass the cloned handle so the caller's binding stays usable elsewhere.
        if let Some(rest) = v.strip_prefix('$')
            && !rest.is_empty()
            && rest.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return format!("{rest}.clone()");
        }
        if v.starts_with('#') {
            return hex_to_color_expr(v);
        }
        if let Ok(n) = v.parse::<f32>() {
            return format_f32(n);
        }
        let snake = to_snake_case(v);
        let in_style = self
            .constants
            .iter()
            .any(|c| to_snake_case(&c.name) == snake);
        let looks_like_color_name = is_ident(v)
            && v.chars()
                .next()
                .is_some_and(|c| c.is_ascii_lowercase() || c == '_');
        if in_style || (self.theme_type.is_some() && looks_like_color_name) {
            return self.color_expr(v);
        }
        // Verbatim pass-through: tag the value with its source span so the analyzer can complete in it.
        let lead = attr.value.len() - attr.value.trim_start().len();
        format!("{}{v}", expr_marker(attr.value_start + lead, v.len()))
    }

    pub(super) fn emit_widget_ref(&mut self, el: &Element) -> ChildEmit {
        let var = el.content.as_deref().unwrap_or("").trim().to_string();
        ChildEmit::Simple {
            name: var,
            code: String::new(),
        }
    }
}
