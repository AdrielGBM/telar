//! Component-call and `widget` reference emitters.

use std::fmt::Write;

use telar_parser::{Attr, Element, Value, ViewNode};

use crate::registry::{Callee, ClosureFate};
use crate::style::{format_f32, hex_to_color_expr};
use telar_project::naming::to_pascal_case;
use telar_project::{AssetContext, IdBaking};

use super::signals::{rust_str, substitute_reads};
use super::{ChildEmit, ChildMode, ViewGen, expr_marker};

impl ViewGen<'_> {
    /// Emits a non-primitive tag as a component call, `tag(TagProps::props()….build())`: one setter per attribute, and one `Children` recipe per prop its markup children are routed to — `.children(…)` for the bare ones, `.header(…)` for those written with `slot:"header"`.
    ///
    /// A prop no child is routed to gets no setter, so children handed to a leaf, or to a slot the callee lacks, are rustc's error on the caller's line: a `.rsx` transpiles knowing nothing but itself, so that is the only place the name can be checked.
    pub(super) fn emit_component_call(&mut self, el: &Element, tag: &str) -> ChildEmit {
        let var = self.next_variable_name("node");
        let pad = self.indent_str();

        let props_attrs: Vec<&Attr> = el
            .attributes
            .iter()
            .filter(|a| is_prop_attr(&a.key))
            .collect();
        let routed = route_children(&el.children);

        if routed.recipes.is_empty() && routed.errors.is_empty() {
            let props_arg = self.component_props_arg(el, tag, &props_attrs, "");
            let code = format!("{pad}let {var} = {tag}({props_arg})?;");
            return ChildEmit::Simple { name: var, code };
        }

        // Built inside a block so its recipe bindings never collide with a sibling call's.
        let mut code = String::new();
        let _ = writeln!(code, "{pad}let {var} = {{");
        self.indent += 1;
        let inner_pad = self.indent_str();
        for (line, message) in &routed.errors {
            let error = format!("{inner_pad}compile_error!({});", rust_str(message));
            let _ = writeln!(code, "{}", super::on_source_line(*line, &error));
        }
        let mut setters = String::new();
        for recipe in &routed.recipes {
            let binding = format!("__recipe_{}", recipe.prop);
            let built = self.emit_recipe(&recipe.nodes);
            let _ = writeln!(
                code,
                "{inner_pad}let {binding} = Children::new(\n{built}\n{inner_pad});"
            );
            let _ = write!(setters, ".{}({binding})", recipe.prop);
        }
        let props_arg = self.component_props_arg(el, tag, &props_attrs, &setters);
        let _ = writeln!(code, "{inner_pad}{tag}({props_arg})?");
        self.indent -= 1;
        let _ = write!(code, "{pad}}};");
        ChildEmit::Simple { name: var, code }
    }

    /// The closure is `Fn` rather than `FnOnce` — a dropdown remakes its rows on every open — so its body is a rebuilding region, which copies what it reads rather than moving it, exactly as a reactive `if`/`for` branch does.
    fn emit_recipe(&mut self, nodes: &[ViewNode]) -> String {
        let pad = self.indent_str();
        let mut body = String::new();
        self.indent += 2;
        let inner_pad = self.indent_str();
        let _ = writeln!(
            body,
            "{inner_pad}let mut __children: Vec<Box<dyn LayoutItem>> = Vec::new();"
        );
        // The callee decides where its children sit, so there is no container here to host a transparent fragment, and the caller's axis must not leak into them.
        self.in_reactive(|g| {
            g.within_host(false, |g| {
                g.with_child_sink(ChildMode::Vec, |g| {
                    for node in nodes {
                        match g.emit_node(node) {
                            ChildEmit::Simple { name, code } => {
                                let _ = writeln!(body, "{code}");
                                let _ =
                                    writeln!(body, "{inner_pad}__children.push(box_item({name}));");
                            }
                            ChildEmit::Dynamic { code } => {
                                let _ = writeln!(body, "{code}");
                            }
                            ChildEmit::Fragment { .. } => {
                                unreachable!("a recipe's children never enter a slot host")
                            }
                        }
                    }
                })
            })
        });
        self.indent -= 2;

        let closure = format!("{pad}    move || {{\n{body}{inner_pad}Ok(__children)\n{pad}    }}");
        let snippets = super::signals::scoped_snippets(nodes, &[]);
        self.owning_subtree(&snippets, closure, &format!("{pad}    "))
    }

    /// Builds a component call's props argument: `NameProps::props()`, one setter per attribute the author wrote, the `recipes` setters, `.build()`.
    ///
    /// **This is where the second type system used to live.** The old form was a `NameProps { … }` literal, which meant the emitter had to know the callee's field types to write each value: eight lists on `ComponentSig` said which props were colours, strings, readings, predicates, owned strings or `Option`s, and whether the struct derived `Default` so the tail could be `..Default::default()`. Every one of those was re-deriving something rustc already knew, and for the shipped catalogue they were hand-mirrored and free to drift.
    ///
    /// A setter answers all of it. `into` on the callee's field decides whether a literal coerces, the field's own type decides whether a `$signal` means the handle or a reading, `Option` needs no `Some(…)` because `From<T> for Option<T>` is std's, and a prop nobody set keeps its declared default. The emitter spells names it read from the markup and knows nothing else.
    ///
    /// The root of a preview reads each of its [`Self::implicit_args`] through the preview's args, so a control can change it, and logs the calls its callbacks receive.
    fn component_props_arg(
        &self,
        el: &Element,
        tag: &str,
        props_attrs: &[&Attr],
        recipes: &str,
    ) -> String {
        let root = self
            .preview_root
            .as_ref()
            .filter(|root| std::ptr::eq(root.element, el));
        let args = match root {
            Some(root) if root.implicit_args => self.implicit_args(el),
            _ => Vec::new(),
        };
        let mut setters: String = props_attrs
            .iter()
            .filter(|attr| class_list(&attr.value).is_none())
            .map(|attr| {
                let value = self
                    .baked_id_expr(tag, attr)
                    .unwrap_or_else(|| self.component_attr_expr(attr));
                let value = match args.iter().any(|arg| std::ptr::eq(*arg, *attr)) {
                    true => format!(
                        "::telar::__preview_arg!(__preview, {}, {value})",
                        rust_str(&attr.key)
                    ),
                    false => value,
                };
                format!(".{}({value})", attr.key)
            })
            .collect();
        setters.push_str(&self.surface_setters(el, props_attrs));
        setters.push_str(recipes);
        if root.is_some() {
            setters.push_str(".__preview_actions(&__preview.actions())");
        }
        format!("{}::props(){setters}.build()", props_type(tag))
    }

    /// The attributes of a preview's root component call that its controls can change: each written as a literal (a string, a number, a flag or a bool, a hex colour, a `Path::Variant`) and not an id the package bakes. A value reading `$state` or a closure stays as written, and so does anything else that is an expression.
    pub(crate) fn implicit_args<'e>(&self, el: &'e Element) -> Vec<&'e Attr> {
        el.attributes
            .iter()
            .filter(|attr| is_prop_attr(&attr.key))
            .filter(|attr| crate::preview::is_literal(&attr.value))
            .filter(|attr| self.baked_id_expr(&el.tag, attr).is_none())
            .collect()
    }

    /// The value of a prop that names an asset by id — `icon name:"mdi:home"` or `icon_button icon:"mdi:home"` where `[telar.icons]` configures icons — or `None` for every other prop, which takes the ordinary emission.
    ///
    /// A literal becomes the `(id, Arc<data>, monochrome)` triple the artifact answers for it where the package bakes it, or the id spelled in full where it resolves at run time, so a bare name is read in the default set here and a bare name with none is a `compile_error!` on its line. A value that is not a literal is a `compile_error!` naming runtime mode where every id must be baked; anywhere else it reaches the component as written, for its runtime source.
    fn baked_id_expr(&self, tag: &str, attr: &Attr) -> Option<String> {
        let name = tag.rsplit("::").next().unwrap_or(tag);
        let assets = self.assets?;
        let resolved = match &attr.value {
            Value::Quoted(id) => assets.literal_id(name, &attr.key, id.trim())?,
            _ => match assets.id_baking(name, &attr.key)? {
                (kind, IdBaking::Required) => {
                    Err(AssetContext::dynamic_id_message(kind, name, &attr.key))
                }
                _ => return None,
            },
        };
        Some(resolved.unwrap_or_else(|message| format!("compile_error!({})", rust_str(&message))))
    }

    /// Every class a component call hands the callee's surfaces, one setter per surface: `sidebar @flat` amends the **principal surface** through `.style(…)`, and `sidebar header_style:@flat` the header **part** through `.header_style(…)`. Several classes stack the same way in both places, the later winning — `sidebar @flat @dense`, `header_style:(@flat @dense)` — the parens being the value's delimiters, as any value holding a space takes them.
    ///
    /// A class list carrying none of the surface properties sets nothing: silently on the principal surface, and on a part with the error [`Self::paintless_part_errors`] gives.
    fn surface_setters(&self, el: &Element, props_attrs: &[&Attr]) -> String {
        let parts = props_attrs.iter().filter_map(|attr| {
            let classes = class_list(&attr.value)?.ok()?;
            crate::registry::is_part_style_key(&attr.key).then_some((attr.key.as_str(), classes))
        });
        std::iter::once(("style", el.classes.clone()))
            .chain(parts)
            .filter_map(|(setter, classes)| {
                let amendment = self.class_surface_style(&classes)?;
                Some(format!(".{setter}({amendment})"))
            })
            .collect()
    }

    /// Why a part would be handed nothing: its classes are all declared and none says anything a surface takes. The principal surface is not held to this, since a class on the call itself may be there for something else.
    pub(super) fn paintless_part_errors(&self, el: &Element) -> Vec<String> {
        if crate::registry::is_builtin_tag(&el.tag) {
            return Vec::new();
        }
        el.attributes
            .iter()
            .filter(|attr| crate::registry::is_part_style_key(&attr.key))
            .filter_map(|attr| Some((attr, class_list(&attr.value)?.ok()?)))
            .filter(|(_, classes)| {
                classes
                    .iter()
                    .all(|name| self.classes.iter().any(|class| &class.name == name))
            })
            .filter(|(_, classes)| self.class_surface_style(classes).is_none())
            .map(|(attr, classes)| {
                format!(
                    "`{}:{}` hands the part nothing: a part takes `{}` from its classes, and `{}` names none of them",
                    attr.key,
                    class_value(&classes),
                    SURFACE_PAINT.join("`, `"),
                    written_classes(&classes),
                )
            })
            .collect()
    }

    /// One surface's classes, compiled into the amendment its setter takes.
    ///
    /// This is the half of styling the DSL used to drop on the floor. An inline attr on a component call is a prop — that meaning is settled and stays. A class was the one thing you could write there that the transpiler parsed, accepted, and then silently ignored: `box @squared` reshapes the box and `menu @squared` reshaped nothing at all, with no error to say why.
    ///
    /// Only the properties the class actually names are applied, as an amendment to the style the component worked out for itself. A class saying `radius:0` must not cost a menu its border, its shadow or its hover fill — rebuilding the whole `RectStyle` the way a `box` does would do exactly that, because a box's style has no author but the class.
    ///
    /// A component with no principal surface — a layout, a fragment, a thing that paints three boxes and can't say which one you meant — has nothing a class could honestly mean. It used to be dropped there, silently, on the word of a table. Now the setter is emitted and rustc says there is no `style` prop, on the author's line: the same answer, given out loud.
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
            let _ = write!(chain, ".with_fill({})", self.color_expr(fill, None));
        }
        if let Some(stroke) = find("stroke") {
            let width = match find("stroke_width") {
                None => "1.0".to_string(),
                Some(w) if crate::edges::tokens(w).len() > 1 => format!(
                    "compile_error!({})",
                    rust_str(&format!(
                        "`stroke_width:{w}` names one width per side, and a class on a component strokes its surface with one"
                    ))
                ),
                Some(w) => crate::style::number_or_error(w, self.theme_access),
            };
            let _ = write!(
                chain,
                ".with_stroke(Stroke::new({}, {width}))",
                self.color_expr(stroke, None),
            );
        }
        if let Some(radius) = find("radius") {
            let _ = write!(
                chain,
                ".with_radius(BorderRadius::all({}))",
                crate::style::format_radius(radius, self.theme_access)
                    .unwrap_or_else(|message| format!("compile_error!({})", rust_str(&message)))
            );
        }
        if chain.is_empty() {
            return None;
        }
        let raw: Vec<&str> = SURFACE_PAINT.iter().filter_map(|key| find(key)).collect();
        let closure = self.owning(&raw, &format!("move |__s: RectStyle| __s{chain}"));
        Some(format!("std::rc::Rc::new({closure})"))
    }

    /// Emits a `children` placeholder, `children $props.header [in:ctx]`: builds the `Children` its expression names where it stands and splices them into the enclosing container's child accumulator, with `ctx` readable to them when `in:` provides one. Dynamic, so the container builds a child vec (see `forces_child_vec`).
    ///
    /// Built here rather than once up front, so the children are made under the owner of the node they are placed in and see that node's `theme:` and every context provided around it, exactly as a child written inline there would. Each placement builds its own.
    pub(super) fn emit_children_placeholder(&mut self, el: &Element) -> ChildEmit {
        let pad = self.indent_str();
        let recipe = match placeholder_recipe(el) {
            Ok(recipe) => self.marked_handles(&recipe.key, recipe.value_start),
            Err(message) => {
                return ChildEmit::Dynamic {
                    code: format!("{pad}compile_error!({});", rust_str(&message)),
                };
            }
        };
        let built = match el.attributes.iter().find(|a| a.key == "in") {
            Some(context) => format!(
                "{recipe}.build_with({})?",
                self.placeholder_context_arg(context)
            ),
            None => format!("{recipe}.build()?"),
        };
        let code = match self.child_sinks.last() {
            Some(sink) if sink.slots => format!(
                "{pad}{}.extend({built}.into_iter().map(ChildSlot::stat));",
                sink.var
            ),
            Some(sink) => format!("{pad}{}.extend({built});", sink.var),
            None => format!("{pad}__children.extend({built});"),
        };
        ChildEmit::Dynamic { code }
    }

    /// `expr`, written at `start`, as the value it names rather than a reading of it: `$props.header` reaches through `props` to the field, as it does anywhere in the view, and is marked so a cursor on it maps to the generated code.
    pub(super) fn marked_handles(&self, expr: &str, start: usize) -> String {
        let substituted = super::signals::substitute_handles(expr, self.theme_access);
        let marked = match expr.strip_prefix('$') {
            Some(rest) if substituted == rest => Some(start + 1),
            Some(_) => None,
            None => Some(start),
        };
        match marked {
            Some(start) => format!("{}{substituted}", expr_marker(start, substituted.len())),
            None => substituted,
        }
    }

    /// The `in:` value a placeholder builds its children inside. A lone `[logic]` binding is cloned wherever the placement may run again or another placement still needs it, as a component prop is.
    fn placeholder_context_arg(&self, attr: &Attr) -> String {
        let text = attr.value.text();
        let lead = text.len() - text.trim_start().len();
        let value = text.trim();
        let (start, expr) = match super::redundant_parens(value) {
            Some(inner) => (attr.value_start + lead + 1, inner),
            None => (attr.value_start + lead, value),
        };
        let marker = expr_marker(start, expr.len());
        let written = format!("{marker}{expr}");
        self.by_value(expr, written.clone()).unwrap_or(written)
    }

    /// Converts a component attribute to a Rust expression:
    /// - a quoted attr (`label:"text"`) becomes a string literal, and a `t"…"` key a catalog lookup;
    /// - a bare flag (`elevated`) or `true`/`false` becomes a `bool`;
    /// - a closure (`on_tap(|| $x += 1)`) becomes a boxed `move` closure with `$signal`s cloned in;
    /// - a lone `$signal` becomes the cloned handle (`count.clone()`);
    /// - numbers become `f32`, hex/named colors resolve via `color_expr`, and anything else (an enum path like `Variant::Primary`, a Rust expression) is forwarded verbatim.
    ///
    /// Simple lowercase identifiers (e.g. `fill:primary`) are routed through `color_expr` so they follow the same [style]-vs-theme precedence as built-in elements. PascalCase or complex expressions are passed through verbatim.
    fn component_attr_expr(&self, attr: &Attr) -> String {
        let v = match &attr.value {
            Value::Quoted(text) => return self.quoted_prop(text, attr.value_start),
            Value::Flag => return "true".to_string(),
            value => value.text().trim(),
        };
        if v == "true" || v == "false" {
            return v.to_string();
        }
        // Boxed like `on_press`, so a `$`-free closure keeps its source span for LSP completion. The delimiting parens come off first: they are the value's delimiters, not part of the closure.
        let (v, delimiters) = match super::redundant_parens(v) {
            Some(inner) => (inner, 1),
            None => (v, 0),
        };
        if v.starts_with('|') || v.starts_with("move |") {
            return format!("std::rc::Rc::new({})", self.emit_closure_value(attr));
        }
        // Pass the cloned handle so the caller's binding stays usable elsewhere.
        if let Some(rest) = v.strip_prefix('$')
            && !rest.is_empty()
            && rest.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return format!("{rest}.clone()");
        }
        if v.starts_with('#') {
            return hex_to_color_expr(v);
        }
        // Handed over untyped, since only the prop knows whether `3` is a `u8` or an `f32`: see `telar::PropNumber`.
        if let Ok(n) = v.parse::<i64>() {
            return format!("::telar::Int({n})");
        }
        if let Ok(n) = v.parse::<f32>() {
            return format!("::telar::Float({})", format_f32(n));
        }
        // Includes the delimiting paren stripped above: `value_start` still points at it, so a span built from `v` would start one byte early and end one short.
        let lead = attr.value.text().len() - attr.value.text().trim_start().len() + delimiters;
        // Cloned only where the clone is load-bearing, since cloning would demand `Clone` of every forwarded binding — which a `Box<dyn Fn()>` prop can never satisfy.
        let written = format!("{}{v}", expr_marker(attr.value_start + lead, v.len()));
        if let Some(taken) = self.by_value(v, written) {
            return taken;
        }
        // A read has to be re-evaluated or it freezes at construction. A lone `$sig` took the arm above and stays a handle, which is what a two-way binding needs.
        let evaluated_now = match self.evaluated_now(v) {
            Ok(evaluated_now) => evaluated_now,
            Err(message) => return format!("compile_error!({})", rust_str(&message)),
        };
        if evaluated_now.contains('$') {
            return reads_state(&self.owning(
                &[v],
                &format!("move || {}", substitute_reads(v, self.theme_access)),
            ));
        }
        // The same read written without the sigil: a hook read through, or a signal's `.get()`. A plain value stays one, evaluated once.
        if self.reads_reactive_context(&evaluated_now) {
            let marker = expr_marker(attr.value_start + lead, v.len());
            return reads_state(&self.owning(&[v], &format!("move || {marker}{v}")));
        }
        // A closure the callee keeps (`Reactive::of(move || …)`) reads when it is called, not here: wrapping it again would nest a `Reactive` in a `Reactive`. It still needs its captures cloned in.
        if v.contains('$') {
            return self.owning(&[v], &substitute_reads(v, self.theme_access));
        }
        if self.reads_reactive_context(v) {
            let marker = expr_marker(attr.value_start + lead, v.len());
            return self.owning(&[v], &format!("{marker}{v}"));
        }
        // The delimiting parens are dropped here rather than at the call, so the span covers the expression and nothing wider.
        match super::redundant_parens(v) {
            Some(inner) => format!(
                "{}{inner}",
                expr_marker(attr.value_start + lead + 1, inner.len())
            ),
            None => format!("{}{v}", expr_marker(attr.value_start + lead, v.len())),
        }
    }
}

impl ViewGen<'_> {
    /// `value` with every closure its callee keeps blanked out, so what is left is what runs when the prop's value is had, the length unchanged so offsets still line up. A closure reading state that is handed to a callee whose behaviour is not fixed is the `Err` asking the markup to say which with `later(…)` or `now(…)`: guessed either way, it freezes silently or is rebuilt on every read.
    fn evaluated_now(&self, value: &str) -> Result<String, String> {
        let handed = crate::lexer::handed_closures(value);
        let mut blanked = value.as_bytes().to_vec();
        let mut kept: Vec<std::ops::Range<usize>> = Vec::new();
        let mut unknown = Vec::new();
        for closure in &handed {
            if kept.iter().any(|k| k.contains(&closure.span.start)) {
                continue;
            }
            match crate::registry::closure_fate(&closure.callee) {
                ClosureFate::Kept => {
                    blanked[closure.span.clone()].fill(b' ');
                    kept.push(closure.span.clone());
                }
                ClosureFate::Now => {}
                ClosureFate::Unknown => unknown.push(closure),
            }
        }
        let blanked = String::from_utf8(blanked).unwrap_or_else(|_| value.to_string());
        let ambiguous = unknown.into_iter().find(|closure| {
            let body = &blanked[closure.span.clone()];
            body.contains('$') || self.reads_reactive_context(body)
        });
        match ambiguous {
            None => Ok(blanked),
            Some(closure) => Err(unknown_closure_fate(value, closure)),
        }
    }
}

/// Why a closure reading state cannot be placed, written as the two spellings that would place it.
fn unknown_closure_fate(value: &str, closure: &crate::lexer::HandedClosure) -> String {
    let call = match &closure.callee {
        Callee::Path(path) => path.clone(),
        Callee::Method(name) => format!(".{name}"),
    };
    let head = value[closure.span.start..closure.body_start].trim_end();
    format!(
        "`{call}(…)` is handed a closure that reads state, and a `.rsx` cannot see whether it keeps the closure or calls it now: write `{call}(later({head} …))` if it keeps it to call later, so the prop is handed over as it is, or `{call}(now({head} …))` if it calls it on the spot, so the prop follows what it reads"
    )
}

impl ViewGen<'_> {
    /// A quoted prop, read as text: a string literal while it has no `{…}` hole, the string formatted once while its holes read nothing that moves, and re-formatted as it moves while one does.
    fn quoted_prop(&self, text: &str, start: usize) -> String {
        match self.interpolation(text, start, false) {
            super::interp::Interpolated::Literal(text) => rust_str(&text),
            super::interp::Interpolated::Formatted { reactive: true, .. } => {
                reads_state(&self.text_closure(text, start))
            }
            super::interp::Interpolated::Formatted { format, .. } => format,
        }
    }
}

/// Wraps a closure that reads state, so the prop follows it instead of freezing at the value it happened to have when the tree was built.
///
/// **The regression this exists to prevent.** A theme read is an `RwSignal` read (`theme-core/src/context.rs`), so `fill:$theme.primary` handed over as a `Color` is the colour the theme had at construction and never moves again — `Reactive::Const` of a snapshot. The old emitter got this right by boxing every prop its `ComponentSig` table called reactive; without the table the rule comes from the value, which is the honest place for it: a `$` is a read wherever it is written.
fn reads_state(closure: &str) -> String {
    format!("Reactive::of({closure})")
}

impl ViewGen<'_> {
    /// `canvas paint:(|rect| …) width:200 height:120` — a leaf the renderer hands its own rect to draw into.
    ///
    /// **This is the tag the `widget` escape existed for.** `Canvas` is a `ui-core` primitive with no tag of its own, so the only way to place one was to build it in `[logic]` and splice the binding — which is also why a `widget` could never sit inside anything that rebuilds: a built widget cannot be made twice. Named as a tag, it is constructed where it is placed, and the question stops arising.
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
        // Deliberately not the prop ladder: that resolves a bare lowercase name against the theme, so a drawing called `draw_paths` came out as a colour constant.
        let text = paint.value.text().trim();
        let closure = match text.starts_with('|') || text.starts_with("move |") {
            true => self.emit_closure_value(paint),
            false => {
                let written = format!("{}{text}", expr_marker(paint.value_start, text.len()));
                self.by_value(text, written.clone()).unwrap_or(written)
            }
        };
        let code = format!("{pad}let {var} = Canvas::new({style}, {closure})?;");
        ChildEmit::Simple { name: var, code }
    }
}

/// The `Props` type belonging to `tag`, which may be a path.
///
/// Only the last segment is the component's name, so `topbar::strip` wants `topbar::StripProps` — Pascal- casing the whole path would ask for a `TopbarstripProps` that exists nowhere. A bare tag keeps resolving however it did: through the crate root today, through an author's `use` once tags are paths.
pub(crate) fn props_type(tag: &str) -> String {
    match tag.rsplit_once("::") {
        Some((module, name)) => format!("{module}::{}Props", to_pascal_case(name)),
        None => to_pascal_case(tag) + "Props",
    }
}

struct Routed {
    recipes: Vec<Recipe>,
    /// A child whose `slot:` names no field, and the line it sits on.
    errors: Vec<(usize, String)>,
}

struct Recipe {
    prop: String,
    nodes: Vec<ViewNode>,
}

/// Bare children go to `children` and a child written with `slot:"header"` to `header`, its `slot:` consumed; the props come in the order they first appear. A view `let` goes into every recipe holding a child after it, since a later sibling in any of them may read it, and a comment into the recipe of the child it precedes.
fn route_children(children: &[ViewNode]) -> Routed {
    let mut errors = Vec::new();
    let props: Vec<Option<String>> = children
        .iter()
        .map(|child| match child {
            ViewNode::Element(el) => match el
                .attributes
                .iter()
                .find(|a| a.key == crate::registry::SLOT_ATTR)
            {
                Some(slot) => slot_prop(slot.value.text())
                    .map_err(|message| errors.push((el.line, message)))
                    .ok(),
                None => Some(CHILDREN_PROP.to_string()),
            },
            ViewNode::IfBlock(_) | ViewNode::ForBlock(_) | ViewNode::MatchBlock(_) => {
                Some(CHILDREN_PROP.to_string())
            }
            ViewNode::LetStmt(_) | ViewNode::Comment(_) => None,
        })
        .collect();

    let mut order: Vec<&String> = Vec::new();
    for prop in props.iter().flatten() {
        if !order.contains(&prop) {
            order.push(prop);
        }
    }
    let next_prop = |from: usize| {
        children[from..]
            .iter()
            .zip(&props[from..])
            .find(|(node, _)| !matches!(node, ViewNode::LetStmt(_) | ViewNode::Comment(_)))
            .and_then(|(_, prop)| prop.as_ref())
    };
    let recipes = order
        .into_iter()
        .map(|prop| {
            let nodes = children
                .iter()
                .enumerate()
                .filter_map(|(at, node)| match node {
                    ViewNode::LetStmt(_) => props[at..]
                        .iter()
                        .any(|later| later.as_ref() == Some(prop))
                        .then(|| node.clone()),
                    ViewNode::Comment(_) => (next_prop(at) == Some(prop)).then(|| node.clone()),
                    ViewNode::Element(el) if props[at].as_ref() == Some(prop) => {
                        let mut routed = el.clone();
                        routed
                            .attributes
                            .retain(|a| a.key != crate::registry::SLOT_ATTR);
                        Some(ViewNode::Element(routed))
                    }
                    _ => (props[at].as_ref() == Some(prop)).then(|| node.clone()),
                })
                .collect();
            Recipe {
                prop: prop.clone(),
                nodes,
            }
        })
        .collect();
    Routed { recipes, errors }
}

const CHILDREN_PROP: &str = "children";

fn slot_prop(name: &str) -> Result<String, String> {
    let name = name.trim();
    match syn::parse_str::<syn::Ident>(name) {
        Ok(_) => Ok(name.to_string()),
        Err(_) => Err(format!(
            "`slot:\"{name}\"` names the prop this child is handed through, so it has to be a Rust field name"
        )),
    }
}

/// The positional expression a `children` placeholder places, or why the line is not one: a placeholder places one `Children` prop, named by its expression.
fn placeholder_recipe(el: &Element) -> Result<&Attr, String> {
    if let Some(name) = el.attributes.iter().find(|a| a.key == "name") {
        let field = name.value.text().trim();
        return Err(format!(
            "`children` places a `Children` prop: write `children $props.{field}`, with `#[props(default)] pub {field}: Children` in `Props`"
        ));
    }
    let mut operands = el.attributes.iter().filter(|a| a.value.is_flag());
    match (operands.next(), operands.next()) {
        (Some(recipe), None) => Ok(recipe),
        (None, _) => Err(
            "`children` places a `Children` prop: write `children $props.children`, with `#[props(default)] pub children: Children` in `Props`"
                .to_string(),
        ),
        (Some(_), Some(extra)) => Err(format!(
            "`children` places one `Children`, and `{}` is a second",
            extra.key
        )),
    }
}

/// The classes a value names, `@flat` or `(@flat @dense)`, or `None` for a value that names none. A value that starts as a class list and is not one is the `Err` saying so.
pub(super) fn class_list(value: &Value) -> Option<Result<Vec<String>, String>> {
    let Value::Expr(text) = value else {
        return None;
    };
    let text = text.trim();
    let listed = super::redundant_parens(text).unwrap_or(text).trim();
    if !listed.starts_with('@') {
        return None;
    }
    let names = listed.split_whitespace().map(|token| {
        token
            .strip_prefix('@')
            .filter(|name| {
                !name.is_empty()
                    && name
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
            })
            .map(str::to_string)
            .ok_or_else(|| {
                format!(
                    "`{text}` is no class list: a part takes `@name`, or `(@first @second)` for several, the later winning"
                )
            })
    });
    Some(names.collect())
}

/// The properties of a class that reach a component's surface.
const SURFACE_PAINT: &[&str] = &["fill", "stroke", "stroke_width", "radius"];

fn written_classes(classes: &[String]) -> String {
    classes
        .iter()
        .map(|name| format!("@{name}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// `classes` spelled as the value of a part's key.
fn class_value(classes: &[String]) -> String {
    match classes.len() {
        1 => written_classes(classes),
        _ => format!("({})", written_classes(classes)),
    }
}

/// Why each attribute of `el` whose value is a class list cannot hand it over: the list is malformed, its key names no part, or `el` is no component call.
pub(super) fn class_value_errors(el: &Element) -> Vec<String> {
    let component = !crate::registry::is_builtin_tag(&el.tag);
    el.attributes
        .iter()
        .filter_map(|attr| {
            let classes = match class_list(&attr.value)? {
                Ok(classes) => classes,
                Err(message) => return Some(message),
            };
            let written = written_classes(&classes);
            let as_value = class_value(&classes);
            let (key, tag) = (&attr.key, &el.tag);
            if component && crate::registry::STATE_STYLE_KEYS.contains(&key.as_str()) {
                None
            } else if !component {
                Some(format!(
                    "`{key}:@…` hands a class to a component's part, and `{tag}` is no component: its classes go on the element, `{tag} {written}`"
                ))
            } else if key == "style" {
                Some(format!(
                    "`style:@…` names the principal surface, which takes its classes on the call itself: `{tag} {written}`"
                ))
            } else if !crate::registry::is_part_style_key(key) {
                Some(format!(
                    "`{key}:@…` hands a class to a part, and a part's prop is named for it, `<part>_style`: `{key}_style:{as_value}`"
                ))
            } else {
                None
            }
        })
        .collect()
}

/// Whether `key` on a component call is one of the callee's props, rather than something the call says about the item it returns or a box's state paint.
fn is_prop_attr(key: &str) -> bool {
    key != crate::registry::SLOT_ATTR
        && !crate::registry::is_component_instance_attr(key)
        && !crate::registry::STATE_STYLE_KEYS.contains(&key)
}

/// Why a state key on a component call names nothing: a component's states are its own, and a class reaches them through the surface it amends.
pub(super) fn state_key_errors(el: &Element) -> Vec<String> {
    if crate::registry::is_builtin_tag(&el.tag) {
        return Vec::new();
    }
    el.attributes
        .iter()
        .filter(|attr| crate::registry::STATE_STYLE_KEYS.contains(&attr.key.as_str()))
        .map(|attr| {
            format!(
                "`{}` paints a state of a `box`, and a component's states are its own: a class restyles them in every state, on the call, `{} @…`, or on a part, `<part>_style:@…`",
                attr.key, el.tag
            )
        })
        .collect()
}

/// `el` without its class-valued attributes, where it is a built-in tag that has some: [`class_value_errors`] reports them, and a `@` reaching the Rust would be a syntax error on no line of the markup.
pub(super) fn without_class_values(el: &Element) -> Option<Element> {
    let stray = |attr: &Attr| class_list(&attr.value).is_some();
    if !crate::registry::is_builtin_tag(&el.tag) || !el.attributes.iter().any(stray) {
        return None;
    }
    let mut stripped = el.clone();
    stripped.attributes.retain(|attr| !stray(attr));
    Some(stripped)
}

/// The classes `el` names in its attribute values, where it is a component call that hands them to its parts.
pub(super) fn part_class_names(el: &Element) -> Vec<String> {
    if crate::registry::is_builtin_tag(&el.tag) {
        return Vec::new();
    }
    el.attributes
        .iter()
        .filter_map(|attr| class_list(&attr.value)?.ok())
        .flatten()
        .collect()
}

/// Whether `attr` on a `children` placeholder is one [`placeholder_recipe`] reads, rather than a key the tag's attribute table answers for: its positional expression, or the retired `name:` it explains.
pub(super) fn is_placeholder_operand(attr: &Attr) -> bool {
    attr.value.is_flag() || attr.key == "name"
}
