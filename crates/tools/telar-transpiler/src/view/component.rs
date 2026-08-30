//! Component-call and `widget` reference emitters.

use std::fmt::Write;

use telar_parser::{Attr, Element, Value, ViewNode};

use crate::naming::to_pascal_case;
use crate::style::{format_f32, hex_to_color_expr};

use super::signals::{rust_str, substitute_reads, wrap_signal_clones};
use super::{ChildEmit, ChildMode, ViewGen, expr_marker};

impl ViewGen<'_> {
    /// Emits a non-primitive tag as a component call: its props builder, then its children as a `Children`
    /// recipe. Always both, whatever the callee looks like.
    ///
    /// **The uniformity is the point.** The emitter used to ask a registry whether the callee took props,
    /// whether it took children, and whether it wanted them built or deferred, then emit one of four call
    /// shapes. Every component takes the same two arguments now, so there is nothing left to ask — and a
    /// callee that wants its children built runs the recipe itself, which is the only place that decision
    /// was ever the callee's to make.
    pub(super) fn emit_component_call(&mut self, el: &Element, tag: &str) -> ChildEmit {
        let var = self.next_variable_name("node");
        let pad = self.indent_str();

        // A `slot:"name"` attr routes THIS element into a parent's named slot; the parent's call-site
        // grouping consumes it, so it is never passed as a prop.
        let props_attrs: Vec<&Attr> = el.attributes.iter().filter(|a| a.key != "slot").collect();
        let has_children = !el.children.is_empty();

        let props_arg = self.component_props_arg(tag, &props_attrs, &el.classes);

        // Childless: the recipe is empty, and the callee still takes one.
        if !has_children {
            let code = format!("{pad}let {var} = {tag}({props_arg}, Children::default())?;");
            return ChildEmit::Simple { name: var, code };
        }

        // Children present: the recipe is built inside a block, so its temp names never collide with a
        // parent's `__children`.
        let mut code = String::new();
        let _ = writeln!(code, "{pad}let {var} = {{");
        self.indent += 1;
        let children_arg = self.emit_deferred_children(&el.children, &mut code);
        let inner_pad = self.indent_str();
        let _ = writeln!(code, "{inner_pad}{tag}({props_arg}, {children_arg})?");
        self.indent -= 1;
        let _ = write!(code, "{pad}}};");
        ChildEmit::Simple { name: var, code }
    }

    /// Emits a compound component's children as a `Children` recipe: the same slot-building body
    /// [`Self::emit_slots`] produces, moved inside a closure the callee runs once it has a context to run it in.
    ///
    /// The closure is `Fn` rather than `FnOnce` — a dropdown remakes its rows on every open — so every signal
    /// it reads is cloned in ahead of it, exactly as a reactive `if`/`for` branch does. Bound to a `let`
    /// first, because the body is statements and a closure body cannot be spliced into an argument position.
    fn emit_deferred_children(&mut self, children: &[ViewNode], code: &mut String) -> String {
        let pad = self.indent_str();
        let mut body = String::new();
        self.indent += 2;
        let slots_expr = self.emit_slots(children, &mut body);
        let inner_pad = self.indent_str();
        self.indent -= 2;

        let closure = format!("{body}{inner_pad}Ok({slots_expr})");
        let idents = super::signals::captured_idents_with(
            &super::signals::subtree_snippets(children)
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            &self.loop_variables,
            &self.locals,
        );
        // Cloned twice on purpose. The outer clone gives the recipe its own handle, so the surrounding view
        // keeps the binding for its other children. The inner one gives *each run* its own, because a body
        // that hands a binding to a widget moves it — and a recipe that can only run once is not a recipe.
        // A signal is `Copy` and neither clone costs anything; the ones that matter are an `Arc` behind an
        // image and a drawing behind a `canvas`.
        let per_run: String = idents
            .iter()
            .map(|name| format!("{pad}        let {name} = {name}.clone();\n"))
            .collect();
        let inner = format!("{pad}    move || {{\n{per_run}{closure}\n{pad}    }}");
        let built = super::signals::clone_block_multiline(&idents, inner, &format!("{pad}    "));
        let _ = writeln!(
            code,
            "{pad}let __deferred = Children::new(\n{built}\n{pad});"
        );
        "__deferred".to_string()
    }

    /// Builds a component call's props argument: `NameProps::props()`, one setter per attribute the author
    /// wrote, `.build()`.
    ///
    /// **This is where the second type system used to live.** The old form was a `NameProps { … }` literal,
    /// which meant the emitter had to know the callee's field types to write each value: eight lists on
    /// `ComponentSig` said which props were colours, strings, readings, predicates, owned strings or
    /// `Option`s, and whether the struct derived `Default` so the tail could be `..Default::default()`. Every
    /// one of those was re-deriving something rustc already knew, and for the shipped catalogue they were
    /// hand-mirrored and free to drift.
    ///
    /// A setter answers all of it. `into` on the callee's field decides whether a literal coerces, the
    /// field's own type decides whether a `$signal` means the handle or a reading, `Option` needs no
    /// `Some(…)` because `From<T> for Option<T>` is std's, and a prop nobody set keeps its declared default.
    /// The emitter spells names it read from the markup and knows nothing else.
    fn component_props_arg(&self, tag: &str, props_attrs: &[&Attr], classes: &[String]) -> String {
        let mut setters: String = props_attrs
            .iter()
            .map(|attr| format!(".{}({})", attr.key, self.component_attr_expr(attr)))
            .collect();
        if let Some(amendment) = self.class_surface_style(classes) {
            let _ = write!(setters, ".style({amendment})");
        }
        format!("{}::props(){setters}.build()", props_type(tag))
    }

    /// A `@class` on a component call, compiled onto the callee's **principal surface**.
    ///
    /// This is the half of styling the DSL used to drop on the floor. An inline attr on a component call is a
    /// prop — that meaning is settled and stays. A class was the one thing you could write there that the
    /// transpiler parsed, accepted, and then silently ignored: `box @squared` reshapes the box and
    /// `menu @squared` reshaped nothing at all, with no error to say why.
    ///
    /// Only the properties the class actually names are applied, as an amendment to the style the component
    /// worked out for itself. A class saying `radius:0` must not cost a menu its border, its shadow or its
    /// hover fill — rebuilding the whole `RectStyle` the way a `box` does would do exactly that, because a
    /// box's style has no author but the class.
    ///
    /// A component with no principal surface — a layout, a fragment, a thing that paints three boxes and
    /// can't say which one you meant — has nothing a class could honestly mean. It used to be dropped there,
    /// silently, on the word of a table. Now the setter is emitted and rustc says there is no `style` prop,
    /// on the author's line: the same answer, given out loud.
    fn class_surface_style(&self, classes: &[String]) -> Option<String> {
        // Reverse order so a later class wins the `find`, matching how `paint_attrs` resolves a box's classes.
        let props: Vec<&telar_parser::StyleProp> = classes
            .iter()
            .rev()
            .filter_map(|name| self.classes.iter().find(|c| &c.name == name))
            .flat_map(|c| c.props.iter())
            .collect();
        let find = |key: &str| {
            props
                .iter()
                .find(|p| p.key == key)
                .map(|p| p.value.as_str())
        };

        let mut chain = String::new();
        if let Some(fill) = find("fill") {
            let _ = write!(chain, ".with_fill({})", self.color_expr(fill));
        }
        if let Some(stroke) = find("stroke") {
            let width = find("stroke_width")
                .and_then(|w| w.parse::<f32>().ok())
                .unwrap_or(1.0);
            let _ = write!(
                chain,
                ".with_stroke(Stroke::new({}, {}))",
                self.color_expr(stroke),
                format_f32(width)
            );
        }
        if let Some(radius) = find("radius") {
            let _ = write!(
                chain,
                ".with_radius(BorderRadius::all({}))",
                crate::style::number_or_error(radius)
            );
        }
        if chain.is_empty() {
            return None;
        }
        let raw: Vec<&str> = [find("fill"), find("stroke")]
            .into_iter()
            .flatten()
            .collect();
        let closure = wrap_signal_clones(&raw, format!("move |__s: RectStyle| __s{chain}"));
        Some(format!("std::rc::Rc::new({closure})"))
    }

    /// Emits the markup children of a component call into a `Slots` value: a child written with
    /// `slot:"name"` goes to that named slot; every other child (including `if`/`for` control flow) goes
    /// to the default slot. Returns the expression naming the built value (`__slots`).
    ///
    /// `slot:` (route, here) and `children name:` (receive, in [`Self::emit_slot`]) deliberately use
    /// different keys — the same route/receive asymmetry as HTML slots and Vue's `<template #x>` vs
    /// `slot="x"`. This is not a naming inconsistency to fix; do not rename either side to match the other.
    fn emit_slots(&mut self, children: &[ViewNode], code: &mut String) -> String {
        let pad = self.indent_str();
        let _ = writeln!(code, "{pad}let mut __slots = Slots::new();");
        let _ = writeln!(
            code,
            "{pad}let mut __children: Vec<Box<dyn LayoutItem>> = Vec::new();"
        );
        // Component call-site children flow into a `Slots` (then the component's own `children` placeholder),
        // so there is no container here to host a transparent fragment: a `Vec` sink named `__children`
        // keeps a reactive `for`/`if` on the boxed path and any nested static control flow pushing there.
        // The callee decides where its `children` placeholder sits, so the caller's axis says nothing about
        // how these will run: don't leak it into the slot bodies.
        self.within_host(false, |g| {
            g.with_child_sink(ChildMode::Vec, |g| {
                for child in children {
                    let slot_name = match child {
                        ViewNode::Element(el) => el
                            .attributes
                            .iter()
                            .find(|a| a.key == "slot")
                            .map(|a| a.value.text().to_string()),
                        _ => None,
                    };
                    // Strip the `slot` attr before emitting a named child, so a component child doesn't receive
                    // it as a prop and a builtin doesn't see a stray attribute.
                    let emit = match (child, &slot_name) {
                        (ViewNode::Element(el), Some(_)) => {
                            let mut stripped = el.clone();
                            stripped.attributes.retain(|a| a.key != "slot");
                            g.emit_node(&ViewNode::Element(stripped))
                        }
                        _ => g.emit_node(child),
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
                                    let _ =
                                        writeln!(code, "{pad}__children.push(box_item({name}));");
                                }
                            }
                        }
                        ChildEmit::Dynamic { code: c } => {
                            let _ = writeln!(code, "{c}");
                        }
                        // Shielded above (`Vec` sink), so a reactive region here is a boxed `ReactiveList`
                        // (a `Simple`), never a fragment.
                        ChildEmit::Fragment { .. } => {
                            unreachable!("component-slot children never enter a slot host")
                        }
                    }
                }
            })
        });
        let _ = writeln!(code, "{pad}__slots.extend_default(__children);");
        "__slots".to_string()
    }

    /// Emits a `children` slot placeholder: splices the caller-supplied children for this slot into the
    /// enclosing container's `__children` vec. `children` drains the default slot; `children name:"x"`
    /// drains the named slot `"x"`. Dynamic, so the container builds a `__children` vec (see `forces_child_vec`).
    ///
    /// Receives via `name:`, the counterpart to the caller's `slot:` in [`Self::emit_slots`] — see that
    /// doc comment for why the two ends intentionally don't share a key.
    pub(super) fn emit_slot(&mut self, el: &Element) -> ChildEmit {
        let pad = self.indent_str();
        let expr = match el.attributes.iter().find(|a| a.key == "name") {
            Some(a) => format!("__slots.take({})", rust_str(a.value.text())),
            None => "__slots.take_default()".to_string(),
        };
        ChildEmit::Dynamic {
            code: format!("{pad}__children.extend({expr});"),
        }
    }

    /// Converts a component attribute to a Rust expression:
    /// - a quoted attr (`label:"text"`) becomes a string literal, and a `t"…"` key a catalog lookup;
    /// - a bare flag (`elevated`) or `true`/`false` becomes a `bool`;
    /// - a closure (`on_tap(|| $x += 1)`) becomes a boxed `move` closure with `$signal`s cloned in;
    /// - a lone `$signal` becomes the cloned handle (`count.clone()`);
    /// - numbers become `f32`, hex/named colors resolve via `color_expr`, and anything else (an enum path
    ///   like `Variant::Primary`, a Rust expression) is forwarded verbatim.
    ///
    /// Simple lowercase identifiers (e.g. `fill:primary`) are routed through `color_expr` so they follow the same [style]-vs-theme precedence as built-in elements. PascalCase or complex expressions are passed through verbatim.
    fn component_attr_expr(&self, attr: &Attr) -> String {
        let v = match &attr.value {
            Value::Quoted(text) => return rust_str(text),
            Value::Flag => return "true".to_string(),
            value => value.text().trim(),
        };
        if v == "true" || v == "false" {
            return v.to_string();
        }
        // Closure prop: box a `move` closure — the same desugaring `on_press` gets, so a `$`-free closure
        // keeps its source span for LSP completion. The Props field is expected to be a `Box<dyn Fn(..)>`.
        //
        // The parens a closure needs to hold its spaces are the value's delimiters, not part of it, so they
        // come off before the shape is read: `on_press:(|| f())` is the same closure as `on_press:|| f()`
        // would be if that could be written at all.
        let v = super::redundant_parens(v).unwrap_or(v);
        if v.starts_with('|') || v.starts_with("move |") {
            return format!("std::rc::Rc::new({})", self.emit_closure_value(attr));
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
        let lead = attr.value.text().len() - attr.value.text().trim_start().len();
        // A lone `[logic]` binding, cloned only where the clone is load-bearing: inside a reactive branch the
        // builder closure runs again and cannot consume its capture, so the binding has to survive the first
        // run. Outside one it is built once and moving is both correct and what the author wrote — cloning
        // there demanded `Clone` of every forwarded binding, which a `Box<dyn Fn()>` prop can never satisfy.
        //
        // `captured_idents` does not cover this: it collects `$signal`s and loop variables, so a plain
        // `[logic]` binding reaches a reactive closure with nothing else keeping it alive.
        // A loop variable counts as a local here. Without that a bare `panel:p` falls through to the style scope and resolves against the theme, so `p` came out as a colour constant — hidden for as long as every call site wrote `.clone()` and took the verbatim path below instead.
        let in_loop = self.loop_variables.iter().any(|l| l == v);
        if self.is_local(v) || in_loop {
            let marker = expr_marker(attr.value_start + lead, v.len());
            // A loop variable is bound once per iteration and the builder closure is `Fn`, so a use that moves it is a use that cannot happen twice.
            return if in_loop || self.must_clone_local(v) {
                format!("{marker}{v}.clone()")
            } else {
                format!("{marker}{v}")
            };
        }
        // A `$` read is a *reading*, not a value: `disabled:$a && $b` and `fill:$theme.primary` have to be
        // re-evaluated or they are the state they read at construction, forever. A lone `$sig` took the arm
        // above and stays a handle, which is what a two-way binding needs.
        if v.contains('$') {
            return reads_state(&wrap_signal_clones(
                &[v],
                format!("move || {}", substitute_reads(v)),
            ));
        }
        // Verbatim pass-through: tag the value with its source span so the analyzer can complete in it. The
        // delimiting parens are dropped here rather than at the call, so the span still covers the expression
        // itself and nothing wider.
        match super::redundant_parens(v) {
            Some(inner) => format!(
                "{}{inner}",
                expr_marker(attr.value_start + lead + 1, inner.len())
            ),
            None => format!("{}{v}", expr_marker(attr.value_start + lead, v.len())),
        }
    }
}

/// Wraps a closure that reads state, so the prop follows it instead of freezing at the value it happened to
/// have when the tree was built.
///
/// **The regression this exists to prevent.** A theme read is an `RwSignal` read
/// (`theme-core/src/context.rs`), so `fill:$theme.primary` handed over as a `Color` is the colour the theme
/// had at construction and never moves again — `Reactive::Const` of a snapshot. The old emitter got this
/// right by boxing every prop its `ComponentSig` table called reactive; without the table the rule comes
/// from the value, which is the honest place for it: a `$` is a read wherever it is written.
fn reads_state(closure: &str) -> String {
    format!("Reactive::of({closure})")
}

impl ViewGen<'_> {
    /// `canvas paint:(|rect| …) width:200 height:120` — a leaf the renderer hands its own rect to draw into.
    ///
    /// **This is the tag the `widget` escape existed for.** `Canvas` is a `ui-core` primitive with no tag of
    /// its own, so the only way to place one was to build it in `[logic]` and splice the binding — which is
    /// also why a `widget` could never sit inside anything that rebuilds: a built widget cannot be made
    /// twice. Named as a tag, it is constructed where it is placed, and the question stops arising.
    pub(super) fn emit_canvas(&mut self, el: &Element) -> ChildEmit {
        let var = self.next_variable_name("canvas");
        let pad = self.indent_str();
        let Some(paint) = el.attributes.iter().find(|a| a.key == "paint") else {
            let msg = "`canvas` needs a `paint:` closure — `canvas paint:(|rect| …)`";
            return ChildEmit::Simple {
                name: format!("compile_error!({})", rust_str(msg)),
                code: String::new(),
            };
        };
        let layout: Vec<Attr> = el
            .attributes
            .iter()
            .filter(|a| a.key != "paint")
            .cloned()
            .collect();
        let style = self.make_layout_style(&el.tag, &el.classes, &layout);
        // A closure written in place is desugared like any other; a name is already the drawing, and it
        // reaches `Canvas` verbatim. Deliberately not the prop ladder: that resolves a bare lowercase name
        // against the theme, so a drawing called `draw_paths` came out as a colour constant.
        let text = paint.value.text().trim();
        let closure = match text.starts_with('|') || text.starts_with("move |") {
            true => self.emit_closure_value(paint),
            false => format!("{}{text}", expr_marker(paint.value_start, text.len())),
        };
        let code = format!("{pad}let {var} = Canvas::new({style}, {closure})?;");
        ChildEmit::Simple { name: var, code }
    }
}

/// The `Props` type belonging to `tag`, which may be a path.
///
/// Only the last segment is the component's name, so `topbar::strip` wants `topbar::StripProps` — Pascal-
/// casing the whole path would ask for a `TopbarstripProps` that exists nowhere. A bare tag keeps resolving
/// however it did: through the crate root today, through an author's `use` once tags are paths.
fn props_type(tag: &str) -> String {
    match tag.rsplit_once("::") {
        Some((module, name)) => format!("{module}::{}Props", to_pascal_case(name)),
        None => to_pascal_case(tag) + "Props",
    }
}
