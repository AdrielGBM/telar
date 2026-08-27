//! Component-call and `widget` reference emitters.

use std::fmt::Write;

use telar_parser::{Attr, Element, Value, ViewNode};

use crate::naming::{is_ident, to_pascal_case, to_snake_case};
use crate::style::{format_f32, hex_to_color_expr};

use super::signals::{rust_str, substitute_reads, wrap_signal_clones};
use super::{ChildEmit, ChildMode, ViewGen, expr_marker};

impl ViewGen<'_> {
    /// Emits an unknown tag as a component function call. A no-attr, no-child tag generates `name()?`;
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
        // call matches its arity, arg count, and optional/reactive-colour props. Owned (a clone), so passing
        // `sig.as_ref()` into the `&self` prop builder never tangles with the later `&mut self` slot calls.
        let sig = self.lookup_component_sig(tag);
        // Pass a `Slots` arg when there are markup children, or when the callee declares a slot (so a
        // childless call still matches its 3-arg signature). Unknown callee → the old "children ⇒ slots".
        let pass_slots = has_children || sig.as_ref().is_some_and(|s| s.has_slot);
        // A compound callee takes the *recipe* for its children rather than the children, so it can build
        // them inside its own context. See `Children` in ui-core for why the order has to invert.
        let defers = sig.as_ref().is_some_and(|s| s.defers_children);

        let props_arg = self.component_props_arg(tag, &props_attrs, &el.classes, sig.as_ref());

        // No children: flat call form. A childless slotted callee still gets an empty second argument.
        if !has_children {
            let empty = if defers {
                "Children::default()"
            } else {
                "Slots::new()"
            };
            let args = Self::call_args(props_arg.as_deref(), pass_slots.then_some(empty));
            let code = format!("{pad}let {var} = {tag}({args})?;");
            return ChildEmit::Simple { name: var, code };
        }

        // Children present: build a `Slots` value inside a block (so the temp names never collide with a
        // parent's `__children`), then pass it as the trailing argument.
        let mut code = String::new();
        let _ = writeln!(code, "{pad}let {var} = {{");
        self.indent += 1;
        let slots_arg = if defers {
            self.emit_deferred_children(&el.children, &mut code)
        } else {
            self.emit_slots(&el.children, &mut code)
        };
        let inner_pad = self.indent_str();
        let args = Self::call_args(props_arg.as_deref(), Some(&slots_arg));
        let _ = writeln!(code, "{inner_pad}{tag}({args})?");
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
        let idents = super::signals::captured_idents(
            &super::signals::subtree_snippets(children)
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            &self.loop_variables,
        );
        let inner = format!("{pad}    move || {{\n{closure}\n{pad}    }}");
        let built = super::signals::clone_block_multiline(&idents, inner, &format!("{pad}    "));
        let _ = writeln!(
            code,
            "{pad}let __deferred = Children::new(\n{built}\n{pad});"
        );
        "__deferred".to_string()
    }

    /// Assembles a component call's argument list: the optional `Props` literal, then the optional trailing
    /// `Slots`. Both the flat and children paths route through here so the arg order stays identical; they
    /// differ only in the slots value (`Slots::new()` when childless vs the built `__slots`).
    fn call_args(props_arg: Option<&str>, slots_arg: Option<&str>) -> String {
        [props_arg, slots_arg]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Builds the `NameProps { … }` argument for a component call, or `None` when no props are needed.
    /// Emits `..Default::default()` only when the callee opts in (its `Props` derives `Default`) and the
    /// call omits some fields, so a full-field call stays literal (no `clippy::needless_update`). When no
    /// props are passed but the callee requires a `Props`, defaults them all. A field the sig marks optional
    /// (its type is `Option<...>`) has its value wrapped in `Some(...)`; omitting it defaults to `None`.
    fn component_props_arg(
        &self,
        tag: &str,
        props_attrs: &[&Attr],
        classes: &[String],
        sig: Option<&crate::codegen::ComponentSig>,
    ) -> Option<String> {
        let callee_has_props = sig.map(|s| s.has_props);
        let props_default = sig.is_some_and(|s| s.props_default);
        let field_count = sig.map(|s| s.prop_fields.len());
        let color_fields: &[String] = sig.map_or(&[], |s| s.color_fields.as_slice());
        let reading_fields: &[String] = sig.map_or(&[], |s| s.reading_fields.as_slice());
        let text_fields: &[String] = sig.map_or(&[], |s| s.text_fields.as_slice());
        let bool_fields: &[String] = sig.map_or(&[], |s| s.bool_fields.as_slice());
        let string_fields: &[String] = sig.map_or(&[], |s| s.string_fields.as_slice());
        let optional_fields: &[String] = sig.map_or(&[], |s| s.optional_fields.as_slice());
        // Bare (not `crate::`) so the type resolves whether the component lives in this crate (via the
        // `use super::*` glob at crate root) or in a component library re-exported through `use telar::*`.
        let props_type = to_pascal_case(tag) + "Props";
        {
            let mut fields: Vec<String> = props_attrs
                .iter()
                .map(|attr| {
                    let value = if color_fields.iter().any(|f| f == &attr.key) {
                        self.component_color_attr_expr(attr)
                    } else if reading_fields.iter().any(|f| f == &attr.key) {
                        self.component_reading_attr_expr(attr)
                    } else if text_fields.iter().any(|f| f == &attr.key) {
                        self.component_text_attr_expr(attr)
                    } else if bool_fields.iter().any(|f| f == &attr.key) {
                        self.component_bool_attr_expr(attr)
                    } else if attr.value.is_quoted() && string_fields.iter().any(|f| f == &attr.key)
                    {
                        // The conversion is what the markup cannot express: a value ends at the first space, so `name:"Box select".to_string()` parses as a second attribute, and the only way out was a `[logic]` local per label.
                        format!("{}.to_string()", rust_str(attr.value.text()))
                    } else {
                        self.component_attr_expr(attr)
                    };
                    // An `Option<...>` prop: wrap the value so a `$signal`, closure, or plain value all fit
                    // (`Some(sig.clone())` / `Some(Box::new(move || …))` / `Some(<expr>)`).
                    let value = if optional_fields.iter().any(|f| f == &attr.key) {
                        format!("Some({value})")
                    } else {
                        value
                    };
                    format!("{}: {}", attr.key, value)
                })
                .collect();
            if let Some(amendment) = self.class_surface_style(classes, sig) {
                fields.push(format!("style: Some({amendment})"));
            }
            if fields.is_empty() {
                // No props passed but the callee has a `Props`: default them all (works when it derives Default).
                return (callee_has_props == Some(true))
                    .then(|| format!("{props_type} {{ ..Default::default() }}"));
            }
            let omits = field_count.is_some_and(|n| fields.len() < n);
            let tail = if props_default && omits {
                ", ..Default::default()"
            } else {
                ""
            };
            Some(format!("{props_type} {{ {}{tail} }}", fields.join(", ")))
        }
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
    /// Gated on the callee declaring a `style` prop. A component with no principal surface — a layout, a
    /// fragment, a thing that paints three boxes and can't say which one you meant — has nothing a class
    /// could honestly mean, so the class is still dropped there rather than guessed at.
    fn class_surface_style(
        &self,
        classes: &[String],
        sig: Option<&crate::codegen::ComponentSig>,
    ) -> Option<String> {
        if !sig.is_some_and(|s| s.prop_fields.iter().any(|f| f == "style")) {
            return None;
        }
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
                self.scope().number_or_error(radius)
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
        Some(format!("Box::new({closure})"))
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
        self.boxed_prop_closure(attr, |s, attr| s.color_expr(attr.value.text()))
    }

    /// A reactive string prop (e.g. a button's `label`): a `move ||` closure re-read every frame, so a
    /// `t"key"` translation re-renders on a locale switch and a `$signal` string re-renders on state change.
    /// Mirrors [`Self::component_color_attr_expr`]; the Props field is expected to be `Box<dyn Fn() -> String>`.
    /// A `t"key"` value becomes a catalog lookup, a plain `"literal"` a static string, and a `$signal`/expr a
    /// reactive read.
    /// A reactive reading prop (a progress bar's `value`): a `move ||` closure re-read every frame, so a
    /// `$signal` follows state and a value derived from several services follows all of them. The Props field
    /// is expected to be `Box<dyn Fn() -> T>`.
    fn component_reading_attr_expr(&self, attr: &Attr) -> String {
        self.boxed_prop_closure(attr, |_, attr| substitute_reads(attr.value.text().trim()))
    }

    fn component_text_attr_expr(&self, attr: &Attr) -> String {
        self.boxed_prop_closure(attr, |s, attr| match &attr.value {
            Value::I18n(key) => s.i18n_lookup(key),
            Value::Quoted(text) => format!("{}.to_string()", rust_str(text)),
            value => substitute_reads(value.text().trim()),
        })
    }

    /// The same for a `Box<dyn Fn() -> bool>` prop — `disabled:$cant_undo`, `checked:$is_on`.
    ///
    /// A predicate rather than a `bool` for the reason the paint props are closures: it is re-read, so a row
    /// that becomes unusable while its menu is open stops being usable then, not at the next rebuild. Any
    /// expression works, not just a bare signal — `!$on`, `$depth == 0` — since the value is spliced with its
    /// `$` reads substituted, exactly as a `box`'s own `disabled:` is.
    fn component_bool_attr_expr(&self, attr: &Attr) -> String {
        self.boxed_prop_closure(attr, |_, attr| {
            // A bare flag (`disabled` with no value) is the attribute asserting itself, as `wrap` and `absolute` do.
            if attr.value.is_flag() {
                "true".to_string()
            } else {
                substitute_reads(attr.value.text().trim())
            }
        })
    }

    /// The shape every reactive component prop shares: an explicit closure passes through, anything else becomes
    /// a `move ||` re-read with its `$signal`s cloned in, boxed for a `Box<dyn Fn() -> T>` field.
    fn boxed_prop_closure(&self, attr: &Attr, body: impl FnOnce(&Self, &Attr) -> String) -> String {
        if let Some(closure) = already_a_closure(attr) {
            return closure;
        }
        let body = body(self, attr);
        // A markup value ends at the first space, so an expression with one in it has to be parenthesised — `active:(a == b)`. Those parens are the syntax's, not the author's, and rustc warns about them if they reach the closure body.
        let body = Self::redundant_parens(&body)
            .map(str::to_string)
            .unwrap_or(body);
        // The closure is `Fn`, so a body that is nothing but a captured binding moves it out on the first call and cannot be called again. What to do about that depends on the binding: a reactive handle is *read*, which is the whole point of the prop being a closure, and anything else is cloned.
        let bare_local = self.is_local(&body) || self.loop_variables.iter().any(|l| l == &body);
        let body = match (
            bare_local,
            self.signal_named_in(&body) == Some(body.as_str()),
        ) {
            (true, true) => format!("{body}.get()"),
            (true, false) => format!("{body}.clone()"),
            _ => body,
        };
        let wrapped = wrap_signal_clones(&[attr.value.text()], format!("move || {body}"));
        format!("Box::new({wrapped})")
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
            Value::I18n(key) => return self.i18n_lookup(key),
            Value::Flag => return "true".to_string(),
            value => value.text().trim(),
        };
        if v == "true" || v == "false" {
            return v.to_string();
        }
        // Closure prop: box a `move` closure — the same desugaring `on_press` gets, so a `$`-free closure
        // keeps its source span for LSP completion. The Props field is expected to be a `Box<dyn Fn(..)>`.
        if v.starts_with('|') {
            return format!("Box::new({})", self.emit_closure_value(attr));
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
        // `theme.…` reads the ambient theme whatever the prop's type: a component takes its sizes and radii from
        // one as readily as its colours, and the verbatim arm below would emit a `theme` the view never bound.
        if !v.contains('$')
            && let Some(expr) = crate::style::theme_field_expr(v, self.theme_type.as_deref())
        {
            return expr;
        }
        let snake = to_snake_case(v);
        let in_style = self
            .constants
            .iter()
            .any(|c| to_snake_case(&c.name) == snake);
        // A bare lowercase name is a theme token only when nothing nearer answers to it. This prop may not even
        // be a colour — the guess reaches every field alike, so `size:size` compiles to a token lookup, and a
        // binding named `radius` or `muted` would read the theme instead of itself without a word.
        let looks_like_color_name = is_ident(v)
            && !self.is_local(v)
            && !self.loop_variables.iter().any(|l| l == v)
            && v.chars()
                .next()
                .is_some_and(|c| c.is_ascii_lowercase() || c == '_');
        if in_style || (self.theme_type.is_some() && looks_like_color_name) {
            return self.color_expr(v);
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
        // Verbatim pass-through: tag the value with its source span so the analyzer can complete in it. The
        // delimiting parens are dropped here rather than at the call, so the span still covers the expression
        // itself and nothing wider.
        match Self::redundant_parens(v) {
            Some(inner) => format!(
                "{}{inner}",
                expr_marker(attr.value_start + lead + 1, inner.len())
            ),
            None => format!("{}{v}", expr_marker(attr.value_start + lead, v.len())),
        }
    }

    /// The expression inside a redundant paren pair, or `None` when the parens carry meaning.
    ///
    /// `key:(expr)` is how the markup delimits a value that would otherwise run to end of line, so the parens
    /// are punctuation rather than grammar and emitting them warns `unused_parens` in code the author cannot
    /// edit. A top-level comma makes them a tuple, which is grammar, and stays.
    fn redundant_parens(expr: &str) -> Option<&str> {
        let inner = expr.trim().strip_prefix('(')?.strip_suffix(')')?;
        let mut depth = 0i32;
        for c in inner.chars() {
            match c {
                '(' | '[' | '{' => depth += 1,
                ')' if depth == 0 => return None,
                ')' | ']' | '}' => depth -= 1,
                ',' if depth == 0 => return None,
                _ => {}
            }
        }
        Some(inner)
    }

    pub(super) fn emit_widget_ref(&mut self, el: &Element) -> ChildEmit {
        let var = el.content.as_deref().unwrap_or("").trim().to_string();
        // `widget "x"` splices `x` as a bare in-scope Rust binding; a non-identifier would emit
        // syntactically broken code, so surface a clear compile error at this element instead. (Semantic
        // "does the binding exist?" / go-to-def / rename remains a future analyzer follow-up — see TODO.)
        if !is_ident(&var) {
            let msg = format!("widget reference \"{var}\" is not a valid Rust identifier");
            return ChildEmit::Simple {
                name: format!("compile_error!({})", rust_str(&msg)),
                code: String::new(),
            };
        }
        // Splicing a binding into a rebuilding region moves the same non-`Clone` `Box<dyn LayoutItem>` twice; rustc catches it as an E0507 against generated code the author never wrote.
        if self.in_reactive_region() {
            let msg = format!(
                "`widget \"{var}\"` cannot be used inside a reactive `if`/`for`: the region rebuilds its \
                 content, and a widget binding can only be placed once. Use `build` with an expression that \
                 constructs it, e.g. build \"{var}()?\"."
            );
            return ChildEmit::Simple {
                name: format!("compile_error!({})", rust_str(&msg)),
                code: String::new(),
            };
        }
        ChildEmit::Simple {
            name: var,
            code: String::new(),
        }
    }

    /// `build "expr"`: splices a Rust *expression* rather than a binding, so it is evaluated afresh at every
    /// construction point — which is what a reactive `if`/`for` needs, since it rebuilds its content each time
    /// a branch or item comes back. Outside a reactive region it behaves exactly like `widget`.
    ///
    /// The expression is emitted verbatim, so a `?` inside it propagates through the enclosing builder (which
    /// returns `Result`), and its identifiers are collected into the reactive closure's clone prelude like any
    /// other snippet — so a signal it reads stays available to the rest of the view.
    pub(super) fn emit_build_expr(&mut self, el: &Element) -> ChildEmit {
        let expr = el.content.as_deref().unwrap_or("").trim().to_string();
        if expr.is_empty() {
            let msg = "`build` needs an expression, e.g. build \"icon_view(name)?\"";
            return ChildEmit::Simple {
                name: format!("compile_error!({})", rust_str(msg)),
                code: String::new(),
            };
        }
        // Not a Rust parser — just enough that a truncated expression names the tag instead of surfacing as a syntax error inside generated code.
        if !delimiters_balanced(&expr) {
            let msg = format!("build expression \"{expr}\" has unbalanced brackets");
            return ChildEmit::Simple {
                name: format!("compile_error!({})", rust_str(&msg)),
                code: String::new(),
            };
        }
        // Emitted bare: every site that splices a child already parenthesises it, so wrapping it again only earns an `unused_parens` warning against generated code.
        ChildEmit::Simple {
            name: expr,
            code: String::new(),
        }
    }
}

/// Whether every `(`/`[`/`{` in `expr` is closed by its own kind, ignoring anything inside a string or char
/// literal — so `build "text(\")\")"` is not misread as unbalanced.
fn delimiters_balanced(expr: &str) -> bool {
    let mut stack = Vec::new();
    let mut chars = expr.chars();
    let mut quote: Option<char> = None;
    while let Some(c) = chars.next() {
        if let Some(open) = quote {
            match c {
                '\\' => {
                    chars.next();
                }
                _ if c == open => quote = None,
                _ => {}
            }
            continue;
        }
        match c {
            '"' | '\'' => quote = Some(c),
            '(' | '[' | '{' => stack.push(c),
            ')' | ']' | '}' => {
                let expected = match c {
                    ')' => '(',
                    ']' => '[',
                    _ => '{',
                };
                if stack.pop() != Some(expected) {
                    return false;
                }
            }
            _ => {}
        }
    }
    stack.is_empty() && quote.is_none()
}

/// A reactive text/colour prop whose value the caller already wrote as a closure, boxed as-is.
///
/// Auto-boxing exists so `label:"Save"` and `tint:$accent` can be written plainly; a caller who wrote
/// `tint(move || fg.get())` has already said what they mean, and wrapping that again yields a closure returning
/// a closure — a type error inside generated code, pointing at a line the author never wrote.
fn already_a_closure(attr: &Attr) -> Option<String> {
    attr.value
        .is_closure()
        .then(|| format!("Box::new({})", attr.value.text().trim()))
}
