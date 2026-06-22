//! Generates the body of the component function from the `[view]` section.

use std::collections::HashMap;
use std::fmt::Write;

use rsx_parser::{Attr, Element, ForBlock, IfBlock, StyleClass, StyleConst, ViewNode};

use crate::naming::{const_name, mentions_ident, style_fn_name, to_pascal_case, to_snake_case};
use crate::signal_scan::SignalInfo;
use crate::style::{format_f32, hex_to_color_expr, layout_prop_call};

/// A piece of generated child code together with how it contributes to a
/// parent's child collection.
enum ChildEmit {
    /// A simple widget bound to `name`, pushable directly.
    Simple { name: String, code: String },
    /// Control flow (`if`/`for`) that mutates a child vector in place.
    Dynamic { code: String },
}

pub struct ViewGen<'a> {
    signals: &'a [SignalInfo],
    /// Declared style classes, used to validate class references in elements.
    classes: &'a [StyleClass],
    constants: &'a [StyleConst],
    /// Per-widget-type variable counters, keyed by the descriptive prefix.
    counters: HashMap<String, usize>,
    /// When set, `[style]` color references resolve to `use_theme::<Type>().field`
    /// instead of generated `COLOR_*` consts, so theme switching takes effect.
    theme_type: Option<String>,
    /// Indentation depth (in 4-space units) for the current emission scope.
    indent: usize,
    /// Loop-variable identifiers currently in scope, cloned per closure like signals.
    loop_vars: Vec<String>,
}

impl<'a> ViewGen<'a> {
    pub fn with_theme(
        signals: &'a [SignalInfo],
        classes: &'a [StyleClass],
        constants: &'a [StyleConst],
        theme_type: Option<&str>,
    ) -> Self {
        Self {
            signals,
            classes,
            constants,
            counters: HashMap::new(),
            theme_type: theme_type.map(str::to_string),
            indent: 1,
            loop_vars: Vec::new(),
        }
    }

    fn next_var(&mut self, tag: &str) -> String {
        let prefix = match tag {
            "text" => "text",
            "btn" | "button" => "btn",
            "col" | "column" => "col",
            "row" => "row",
            "box" => "sbox",
            "rect" => "rect",
            "img" | "image" => "img",
            "canvas" => "canvas",
            _ => "node",
        };
        let count = self.counters.entry(prefix.to_string()).or_insert(0);
        let name = format!("__{prefix}_{count}");
        *count += 1;
        name
    }

    fn pad(&self) -> String {
        "    ".repeat(self.indent)
    }

    /// Generates the full view body and returns the final `Ok(Box::new(...))` expression.
    pub fn generate_root(&mut self, nodes: &[ViewNode]) -> String {
        let mut out = String::new();
        let mut last_widget: Option<String> = None;
        let mut roots = Vec::new();

        for node in nodes {
            match self.emit_node(node) {
                ChildEmit::Simple { name, code } => {
                    out.push_str(&code);
                    out.push('\n');
                    last_widget = Some(name.clone());
                    roots.push(name);
                }
                ChildEmit::Dynamic { code } => {
                    // A bare control-flow node at the root has no container to
                    // attach to; emit it verbatim for completeness.
                    out.push_str(&code);
                    out.push('\n');
                }
            }
        }

        let pad = self.pad();
        match roots.len() {
            0 => {
                let _ = write!(
                    out,
                    "{pad}Ok(Box::new(Container::column(ctx, children![])?))"
                );
            }
            1 => {
                let only = last_widget.unwrap_or_else(|| roots[0].clone());
                let _ = write!(out, "{pad}Ok(Box::new({only}))");
            }
            _ => {
                let items = roots.join(", ");
                let _ = write!(
                    out,
                    "{pad}Ok(Box::new(Container::column(ctx, children![{items}])?))"
                );
            }
        }

        out
    }

    fn emit_node(&mut self, node: &ViewNode) -> ChildEmit {
        match node {
            ViewNode::Element(el) => self.emit_element(el),
            ViewNode::LetStmt { source, .. } => ChildEmit::Dynamic {
                code: format!("{}{source};", self.pad()),
            },
            ViewNode::IfBlock(block) => self.emit_if(block),
            ViewNode::ForBlock(block) => self.emit_for(block),
        }
    }

    fn emit_element(&mut self, el: &Element) -> ChildEmit {
        match el.tag.as_str() {
            "text" => self.emit_text(el),
            "btn" => self.emit_button(el),
            "col" | "row" | "grid" => self.emit_container(el),
            "box" => self.emit_box(el),
            "img" | "image" => self.emit_image(el),
            "scroll" => self.emit_scroll(el),
            "canvas" => self.emit_canvas(el),
            "widget" => self.emit_widget_ref(el),
            other => self.emit_component_call(el, other),
        }
    }

    // ----- Leaf widgets -----------------------------------------------------

    fn emit_text(&mut self, el: &Element) -> ChildEmit {
        let var = self.next_var(&el.tag);
        let pad = self.pad();
        let content = el.content.as_deref().unwrap_or("");
        let content_fn = self.interpolate_content(content);
        let style = self.text_style(&el.attrs);

        let font_size = el
            .attrs
            .iter()
            .find(|a| a.key == "size")
            .and_then(|a| a.value.parse::<f32>().ok())
            .unwrap_or(14.0);
        // Emit the multiplication at runtime so the generated source stays free
        // of f32 rounding noise (`25.199999`) from precomputing line-height.
        let font_size = format_f32(font_size);
        let width_call = el
            .attrs
            .iter()
            .find(|a| a.key == "width")
            .and_then(|a| layout_prop_call("width", &a.value))
            .unwrap_or_default();
        let layout_style = format!("LayoutStyle::new().height({font_size}_f32 * 1.4){width_call}");

        // Each `move` closure consumes its captures; clone the signals they use
        // into block locals so both closures can capture independently.
        let clones = self.clone_bindings(&[&content_fn, &style], &pad, "    ");

        let code = format!(
            "{pad}let {var} = {{\n\
             {clones}\
             {pad}    Text::new(\n\
             {pad}        ctx,\n\
             {pad}        {content_fn},\n\
             {pad}        {layout_style},\n\
             {pad}        {style},\n\
             {pad}    )?\n\
             {pad}}};"
        );
        ChildEmit::Simple { name: var, code }
    }

    /// Emits `let sig = sig.clone();` bindings for every signal that appears in
    /// any of `snippets`, indented under `pad + extra`.
    fn clone_bindings(&self, snippets: &[&str], pad: &str, extra: &str) -> String {
        let mut used: Vec<&str> = Vec::new();
        for sig in self.signals {
            if snippets.iter().any(|s| mentions_ident(s, &sig.name)) {
                used.push(&sig.name);
            }
        }
        for var in &self.loop_vars {
            if snippets.iter().any(|s| mentions_ident(s, var)) {
                used.push(var);
            }
        }
        let mut out = String::new();
        for name in used {
            let _ = writeln!(out, "{pad}{extra}let {name} = {name}.clone();");
        }
        out
    }

    fn text_style(&self, attrs: &[Attr]) -> String {
        let size = attrs
            .iter()
            .find(|a| a.key == "size")
            .and_then(|a| a.value.parse::<f32>().ok())
            .map(format_f32)
            .unwrap_or_else(|| "14.0".to_string());
        let color = attrs
            .iter()
            .find(|a| a.key == "color")
            .map(|a| self.color_expr(&a.value))
            .unwrap_or_else(|| "Color::BLACK".to_string());
        format!("move || TextStyle::new({size}, {color})")
    }

    fn emit_image(&mut self, el: &Element) -> ChildEmit {
        let var = self.next_var("img");
        let pad = self.pad();

        let src = el
            .attrs
            .iter()
            .find(|a| a.key == "src")
            .map(|a| a.value.as_str())
            .unwrap_or("__img_data");

        let filter = el
            .attrs
            .iter()
            .find(|a| a.key == "filter")
            .map(|a| match a.value.trim() {
                "Nearest" | "nearest" => "ImageFilter::Nearest",
                _ => "ImageFilter::Linear",
            })
            .unwrap_or("ImageFilter::Linear");

        let layout_style = self.make_layout_style("img", &el.classes, &el.attrs);

        let code = format!(
            "{pad}let {var} = {{\n\
             {pad}    let __src = {src}.clone();\n\
             {pad}    Image::new(\n\
             {pad}        ctx,\n\
             {pad}        {layout_style},\n\
             {pad}        move || __src.clone(),\n\
             {pad}        move || {filter},\n\
             {pad}    )?\n\
             {pad}}};"
        );

        ChildEmit::Simple { name: var, code }
    }

    fn emit_button(&mut self, el: &Element) -> ChildEmit {
        let var = self.next_var(&el.tag);
        let pad = self.pad();
        let label = el.content.as_deref().unwrap_or("");
        let style = self.button_style(&el.attrs, pad.as_str());

        let on_press = el
            .attrs
            .iter()
            .find(|a| a.key == "on_press")
            .map(|h| normalize_closure(&h.value));

        let mut snippets: Vec<&str> = Vec::new();
        if let Some(s) = &style {
            snippets.push(s);
        }
        if let Some(c) = &on_press {
            snippets.push(c);
        }
        let clones = self.clone_bindings(&snippets, &pad, "    ");

        let mut code = String::new();
        let _ = writeln!(code, "{pad}let {var} = {{");
        code.push_str(&clones);
        let _ = writeln!(
            code,
            "{pad}    let mut __btn = Button::new(ctx, {})?;",
            rust_str(label)
        );
        if let Some(style) = style {
            let _ = writeln!(code, "{pad}    __btn = __btn.style({style});");
        }
        if let Some(closure) = on_press {
            let _ = writeln!(code, "{pad}    __btn = __btn.on_click(move {closure});");
        }
        let _ = writeln!(code, "{pad}    __btn");
        let _ = write!(code, "{pad}}};");

        ChildEmit::Simple { name: var, code }
    }

    /// Builds the `ButtonStyle` closure, or `None` if no styling attributes are present.
    fn button_style(&self, attrs: &[Attr], pad: &str) -> Option<String> {
        let fill = attrs.iter().find(|a| a.key == "fill");
        let outline = attrs.iter().find(|a| a.key == "outline");
        let ghost = attrs.iter().any(|a| a.key == "ghost");

        if fill.is_none() && outline.is_none() && !ghost {
            return None;
        }

        let radius = "BorderRadius::all(4.0)";

        let (rect, rect_hover, text_style, text_hover_style) = if let Some(fill) = fill {
            let c = self.color_expr(&fill.value);
            (
                format!("RectStyle::default().with_fill({c}).with_radius({radius})"),
                format!("RectStyle::default().with_fill({c}).with_radius({radius})"),
                "TextStyle::new(14.0, Color::WHITE)".to_string(),
                "TextStyle::new(14.0, Color::WHITE)".to_string(),
            )
        } else if let Some(outline) = outline {
            let c = self.color_expr(&outline.value);
            (
                format!(
                    "RectStyle::default().with_stroke(Stroke::new({c}, 1.5)).with_radius({radius})"
                ),
                format!("RectStyle::default().with_fill({c}).with_radius({radius})"),
                format!("TextStyle::new(14.0, {c})"),
                "TextStyle::new(14.0, Color::WHITE)".to_string(),
            )
        } else {
            // ghost: transparent in both states, dark neutral text.
            let ghost_text = "TextStyle::new(14.0, Color::rgba(0.15, 0.15, 0.2, 1.0))".to_string();
            (
                format!("RectStyle::default().with_radius({radius})"),
                format!("RectStyle::default().with_radius({radius})"),
                ghost_text.clone(),
                ghost_text,
            )
        };

        Some(format!(
            "move || ButtonStyle {{\n\
             {pad}        rect: {rect},\n\
             {pad}        rect_hover: {rect_hover},\n\
             {pad}        text: {text_style},\n\
             {pad}        text_hover: {text_hover_style},\n\
             {pad}    }}"
        ))
    }

    // ----- Containers -------------------------------------------------------

    fn emit_container(&mut self, el: &Element) -> ChildEmit {
        let var = self.next_var(&el.tag);
        let pad = self.pad();
        let style = self.make_layout_style(&el.tag, &el.classes, &el.attrs);

        // Partition children into simple widgets and dynamic control flow.
        let has_dynamic = el.children.iter().any(|n| {
            matches!(
                n,
                ViewNode::IfBlock(_) | ViewNode::ForBlock(_) | ViewNode::LetStmt { .. }
            )
        });

        self.indent += 1;
        let inner_pad = self.pad();
        let mut child_emits = Vec::new();
        for child in &el.children {
            child_emits.push(self.emit_node(child));
        }
        self.indent -= 1;

        let mut code = String::new();
        let _ = writeln!(code, "{pad}let {var} = {{");

        if has_dynamic {
            let _ = writeln!(
                code,
                "{inner_pad}let mut __children: Vec<Box<dyn LayoutItem>> = Vec::new();"
            );
            for emit in &child_emits {
                match emit {
                    ChildEmit::Simple { name, code: c } => {
                        let _ = writeln!(code, "{c}");
                        let _ = writeln!(
                            code,
                            "{inner_pad}__children.push(Box::new({name}) as Box<dyn LayoutItem>);"
                        );
                    }
                    ChildEmit::Dynamic { code: c } => {
                        let _ = writeln!(code, "{c}");
                    }
                }
            }
            let _ = writeln!(code, "{inner_pad}Container::new(ctx, {style}, __children)?");
        } else {
            let mut names = Vec::new();
            for emit in &child_emits {
                if let ChildEmit::Simple { name, code: c } = emit {
                    let _ = writeln!(code, "{c}");
                    names.push(name.clone());
                } else if let ChildEmit::Dynamic { code: c } = emit {
                    let _ = writeln!(code, "{c}");
                }
            }
            let items = names.join(", ");
            let _ = writeln!(
                code,
                "{inner_pad}Container::new(ctx, {style}, children![{items}])?"
            );
        }

        let _ = write!(code, "{pad}}};");
        ChildEmit::Simple { name: var, code }
    }

    fn emit_box(&mut self, el: &Element) -> ChildEmit {
        let var = self.next_var("box");
        let pad = self.pad();
        let layout_style = self.make_layout_style("box", &el.classes, &el.attrs);

        let shadow = self.canvas_shadow(&el.attrs);
        let gradient = self.box_gradient_paint(&el.attrs);
        let solid_fill = el
            .attrs
            .iter()
            .find(|a| a.key == "fill")
            .map(|a| self.color_expr(&a.value));
        let stroke = el
            .attrs
            .iter()
            .find(|a| a.key == "stroke")
            .map(|a| self.color_expr(&a.value));
        let stroke_w = el
            .attrs
            .iter()
            .find(|a| a.key == "stroke_w")
            .and_then(|a| a.value.parse::<f32>().ok())
            .unwrap_or(1.0);
        let radius = el
            .attrs
            .iter()
            .find(|a| a.key == "radius")
            .and_then(|a| a.value.parse::<f32>().ok())
            .map(|r| format!("BorderRadius::all({})", format_f32(r)))
            .unwrap_or_else(|| "BorderRadius::zero()".to_string());
        let opacity = el
            .attrs
            .iter()
            .find(|a| a.key == "opacity")
            .and_then(|a| a.value.parse::<f32>().ok());

        // Gradient needs the rendered rect via closure param; others don't.
        let uses_r = gradient.is_some();
        let param = if uses_r { "r" } else { "_" };

        let rect_style = build_rect_style(gradient, solid_fill, stroke, stroke_w, shadow, &radius);

        let opacity_call = opacity
            .map(|o| format!(".with_opacity({})", format_f32(o)))
            .unwrap_or_default();

        let has_dynamic = el.children.iter().any(|n| {
            matches!(
                n,
                ViewNode::IfBlock(_) | ViewNode::ForBlock(_) | ViewNode::LetStmt { .. }
            )
        });

        self.indent += 1;
        let inner_pad = self.pad();
        let mut child_emits = Vec::new();
        for child in &el.children {
            child_emits.push(self.emit_node(child));
        }
        self.indent -= 1;

        let mut code = String::new();
        let _ = writeln!(code, "{pad}let {var} = {{");

        if has_dynamic {
            let _ = writeln!(
                code,
                "{inner_pad}let mut __children: Vec<Box<dyn LayoutItem>> = Vec::new();"
            );
            for emit in &child_emits {
                match emit {
                    ChildEmit::Simple { name, code: c } => {
                        let _ = writeln!(code, "{c}");
                        let _ = writeln!(
                            code,
                            "{inner_pad}__children.push(Box::new({name}) as Box<dyn LayoutItem>);"
                        );
                    }
                    ChildEmit::Dynamic { code: c } => {
                        let _ = writeln!(code, "{c}");
                    }
                }
            }
            let _ = writeln!(
                code,
                "{inner_pad}StyledContainer::new(ctx, {layout_style}, move |{param}| {rect_style}, __children)?{opacity_call}"
            );
        } else {
            let mut names = Vec::new();
            for emit in &child_emits {
                if let ChildEmit::Simple { name, code: c } = emit {
                    let _ = writeln!(code, "{c}");
                    names.push(name.clone());
                } else if let ChildEmit::Dynamic { code: c } = emit {
                    let _ = writeln!(code, "{c}");
                }
            }
            let items = names.join(", ");
            let _ = writeln!(
                code,
                "{inner_pad}StyledContainer::new(ctx, {layout_style}, move |{param}| {rect_style}, children![{items}])?{opacity_call}"
            );
        }

        let _ = write!(code, "{pad}}};");
        ChildEmit::Simple { name: var, code }
    }

    /// Builds a `Paint::Gradient(...)` expression for a `box` element, using the
    /// closure parameter `r` (the rendered `Bounds`) for absolute gradient points.
    ///
    /// `gradient:horizontal/vertical/diagonal/radial` with `from:` / `to:` (required),
    /// optional `mid:` / `mid-pos:`.
    fn box_gradient_paint(&self, attrs: &[Attr]) -> Option<String> {
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
            .find(|a| a.key == "mid-pos")
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
                    .map(|n| format_f32(n))
                    .unwrap_or_else(|| "r.width.min(r.height) * 0.5".to_string());
                Some(format!(
                    "Paint::Gradient(Gradient::radial(Point::new(r.x + r.width * 0.5, r.y + r.height * 0.5), {radius_expr}, {stops}))"
                ))
            }
            _ => None,
        }
    }

    fn emit_scroll(&mut self, el: &Element) -> ChildEmit {
        let var = self.next_var(&el.tag);
        let pad = self.pad();
        let style = self.make_layout_style(&el.tag, &el.classes, &el.attrs);

        self.indent += 1;
        let inner_pad = self.pad();
        // ScrollArea wraps a single content item; if multiple children exist,
        // wrap them in a column first.
        let mut child_emits = Vec::new();
        for child in &el.children {
            child_emits.push(self.emit_node(child));
        }
        self.indent -= 1;

        let mut code = String::new();
        let _ = writeln!(code, "{pad}let {var} = {{");

        let has_dynamic = child_emits
            .iter()
            .any(|e| matches!(e, ChildEmit::Dynamic { .. }));

        if has_dynamic {
            let _ = writeln!(
                code,
                "{inner_pad}let mut __children: Vec<Box<dyn LayoutItem>> = Vec::new();"
            );
            for emit in &child_emits {
                match emit {
                    ChildEmit::Simple { name, code: c } => {
                        let _ = writeln!(code, "{c}");
                        let _ = writeln!(
                            code,
                            "{inner_pad}__children.push(Box::new({name}) as Box<dyn LayoutItem>);"
                        );
                    }
                    ChildEmit::Dynamic { code: c } => {
                        let _ = writeln!(code, "{c}");
                    }
                }
            }
            let _ = writeln!(
                code,
                "{inner_pad}let __scroll_content = Container::column(ctx, __children)?;"
            );
            let _ = writeln!(
                code,
                "{inner_pad}LayoutScrollArea::new(ctx, {style}, Box::new(__scroll_content))?"
            );
        } else {
            let mut names = Vec::new();
            for emit in &child_emits {
                if let ChildEmit::Simple { name, code: c } = emit {
                    let _ = writeln!(code, "{c}");
                    names.push(name.clone());
                }
            }

            let content = match names.len() {
                0 => "Container::column(ctx, children![])?".to_string(),
                1 => names.remove(0),
                _ => {
                    let items = names.join(", ");
                    format!("Container::column(ctx, children![{items}])?")
                }
            };

            let _ = writeln!(
                code,
                "{inner_pad}LayoutScrollArea::new(ctx, {style}, Box::new({content}))?"
            );
        }

        let _ = write!(code, "{pad}}};");
        ChildEmit::Simple { name: var, code }
    }

    fn emit_canvas(&mut self, el: &Element) -> ChildEmit {
        let var = self.next_var(&el.tag);
        let pad = self.pad();
        let style = self.make_layout_style(&el.tag, &el.classes, &el.attrs);
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

        let mut code = String::new();
        let _ = writeln!(
            code,
            "{pad}let {var} = Canvas::new(ctx, {style}, move |__rect| {{"
        );

        if canvas_children.is_empty() {
            // Legacy behaviour: explicit (w, h) param bindings or empty stub.
            let params = el.canvas_params.as_deref().unwrap_or("");
            let bindings = canvas_param_bindings(params, &pad);
            code.push_str(&bindings);
            let _ = writeln!(code, "{inner}RenderNode::group([])");
        } else {
            // Inject dimension locals so children can write w:full / h:full.
            let _ = writeln!(code, "{inner}let __w = __rect.width;");
            let _ = writeln!(code, "{inner}let __h = __rect.height;");
            let exprs: Vec<String> = canvas_children
                .iter()
                .map(|child| self.emit_render_node_expr(child))
                .collect();
            if exprs.len() == 1 {
                let _ = writeln!(code, "{inner}{}", exprs[0]);
            } else {
                let _ = writeln!(code, "{inner}RenderNode::group([");
                for expr in &exprs {
                    let _ = writeln!(code, "{inner}    {expr},");
                }
                let _ = writeln!(code, "{inner}])");
            }
        }

        let _ = write!(code, "{pad}}})?;");
        ChildEmit::Simple { name: var, code }
    }

    /// Emits a `RenderNode` expression for an element that is a direct child of
    /// a `canvas` element. The result is an expression string, not a statement.
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
    /// Attrs: `x`, `y`, `w`, `h` (numbers or `full`), `fill`, `stroke`,
    /// `stroke_w`, `radius`, `shadow-x`, `shadow-y`, `shadow-blur`, `shadow-color`,
    /// `gradient` (linear/radial), `from`, `to`, `mid`, `mid-pos`,
    /// `x1`, `y1`, `x2`, `y2` (linear points), `cx`, `cy`, `r` (radial).
    fn emit_canvas_rect(&self, el: &Element) -> String {
        let x = self.canvas_dim("x", &el.attrs);
        let y = self.canvas_dim("y", &el.attrs);
        let w = self.canvas_dim("w", &el.attrs);
        let h = self.canvas_dim("h", &el.attrs);

        let radius = el
            .attrs
            .iter()
            .find(|a| a.key == "radius")
            .and_then(|a| a.value.parse::<f32>().ok())
            .map(|r| format!("BorderRadius::all({})", format_f32(r)))
            .unwrap_or_else(|| "BorderRadius::zero()".to_string());

        let shadow = self.canvas_shadow(&el.attrs);
        let stroke = el
            .attrs
            .iter()
            .find(|a| a.key == "stroke")
            .map(|a| self.color_expr(&a.value));
        let stroke_w = el
            .attrs
            .iter()
            .find(|a| a.key == "stroke_w")
            .and_then(|a| a.value.parse::<f32>().ok())
            .unwrap_or(1.0);
        let gradient = self.canvas_gradient_paint(&el.attrs);
        let solid_fill = el
            .attrs
            .iter()
            .find(|a| a.key == "fill")
            .map(|a| self.color_expr(&a.value));

        let rect_style = build_rect_style(gradient, solid_fill, stroke, stroke_w, shadow, &radius);

        format!(
            "RenderNode::rect(Rect {{ x: {x}, y: {y}, width: {w}, height: {h} }}, {rect_style})"
        )
    }

    /// Builds a `Paint::Gradient(...)` expression when `gradient:linear` or
    /// `gradient:radial` is present. Color stops: `from:` / `to:` (required),
    /// optional `mid:` with `mid-pos:` (default 0.5).
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
            .find(|a| a.key == "mid-pos")
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

    /// Extracts `shadow-*` attrs and produces a `Some(Shadow::new(...))` expression,
    /// or `None` when no shadow attrs are present.
    fn canvas_shadow(&self, attrs: &[Attr]) -> Option<String> {
        if !attrs.iter().any(|a| a.key.starts_with("shadow")) {
            return None;
        }
        let sx = attrs
            .iter()
            .find(|a| a.key == "shadow-x")
            .and_then(|a| a.value.parse::<f32>().ok())
            .unwrap_or(0.0);
        let sy = attrs
            .iter()
            .find(|a| a.key == "shadow-y")
            .and_then(|a| a.value.parse::<f32>().ok())
            .unwrap_or(4.0);
        let blur = attrs
            .iter()
            .find(|a| a.key == "shadow-blur")
            .and_then(|a| a.value.parse::<f32>().ok())
            .unwrap_or(8.0);
        let color = attrs
            .iter()
            .find(|a| a.key == "shadow-color")
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
    /// Attrs: `x1`, `y1`, `x2`, `y2` (coordinates), `color`, `width`/`stroke_w`.
    fn emit_canvas_line(&self, el: &Element) -> String {
        let coord = |key: &str| -> String {
            el.attrs
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
            .attrs
            .iter()
            .find(|a| a.key == "color")
            .map(|a| self.color_expr(&a.value))
            .unwrap_or_else(|| "Color::BLACK".to_string());
        let width = el
            .attrs
            .iter()
            .find(|a| a.key == "width" || a.key == "stroke_w")
            .and_then(|a| a.value.parse::<f32>().ok())
            .map(format_f32)
            .unwrap_or_else(|| "1.0".to_string());
        format!(
            "Line::new(move || Point::new({x1}, {y1}), move || Point::new({x2}, {y2}), move || LineStyle::new({color}, {width})).view()"
        )
    }

    /// Generates a `RenderNode::layer(opacity, blur, [...])` expression.
    ///
    /// Attrs: `opacity` (default 1.0), `blur` (default 0.0).
    /// Children are recursively emitted as canvas render-node expressions.
    fn emit_canvas_layer(&self, el: &Element) -> String {
        let opacity = el
            .attrs
            .iter()
            .find(|a| a.key == "opacity")
            .and_then(|a| a.value.parse::<f32>().ok())
            .map(format_f32)
            .unwrap_or_else(|| "1.0".to_string());
        let blur = el
            .attrs
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

    /// Generates a `RenderNode::text(...)` expression for a `text` element inside
    /// a canvas. Uses absolute coordinates unlike layout-mode `text`.
    fn emit_canvas_text(&self, el: &Element) -> String {
        let content = el.content.as_deref().unwrap_or("");
        let x = self.canvas_dim("x", &el.attrs);
        let y = self.canvas_dim("y", &el.attrs);
        let w = self.canvas_dim("w", &el.attrs);
        let h = self.canvas_dim("h", &el.attrs);

        let size = el
            .attrs
            .iter()
            .find(|a| a.key == "size")
            .and_then(|a| a.value.parse::<f32>().ok())
            .map(format_f32)
            .unwrap_or_else(|| "14.0".to_string());

        let color = el
            .attrs
            .iter()
            .find(|a| a.key == "color")
            .map(|a| self.color_expr(&a.value))
            .unwrap_or_else(|| "Color::BLACK".to_string());

        format!(
            "RenderNode::text({}, Rect {{ x: {x}, y: {y}, width: {w}, height: {h} }}, TextStyle::new({size}, {color}))",
            rust_str(content)
        )
    }

    /// Resolves a canvas dimension attribute (`x`, `y`, `w`, `h`).
    /// `"full"` maps to `__w` (width axis) or `__h` (height axis).
    /// Omitted `w`/`h` default to `__w`/`__h`; omitted `x`/`y` default to `0.0`.
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

    /// Emits an unknown tag as a component function call. No-attr tags generate
    /// `name(ctx)?`; tags with attrs generate a `NameProps { … }` struct literal.
    /// The component's `.rsx` file must declare a matching `pub struct NameProps`.
    fn emit_component_call(&mut self, el: &Element, tag: &str) -> ChildEmit {
        let var = self.next_var("node");
        let pad = self.pad();

        if el.attrs.is_empty() && el.children.is_empty() {
            let code = format!("{pad}let {var} = {tag}(ctx)?;");
            return ChildEmit::Simple { name: var, code };
        }

        let props_type = to_pascal_case(tag) + "Props";
        let fields: Vec<String> = el
            .attrs
            .iter()
            .map(|attr| format!("{}: {}", attr.key, self.component_attr_expr(attr)))
            .collect();
        let code = format!(
            "{pad}let {var} = {tag}(ctx, crate::{props_type} {{ {} }})?;",
            fields.join(", ")
        );
        ChildEmit::Simple { name: var, code }
    }

    /// Converts a component attribute to a Rust expression. Quoted attrs (`label:"text"`)
    /// become string literals; numbers become `f32` literals; hex/named colors resolve
    /// via `color_expr`; everything else is forwarded verbatim.
    ///
    /// Simple lowercase identifiers (e.g. `fill:primary`) are routed through
    /// `color_expr` so they follow the same [style]-vs-theme precedence as built-in
    /// elements. PascalCase or complex expressions are passed through verbatim.
    fn component_attr_expr(&self, attr: &Attr) -> String {
        if attr.is_quoted {
            return rust_str(&attr.value);
        }
        let v = attr.value.trim();
        if v.starts_with('#') {
            return crate::style::hex_to_color_expr(v);
        }
        if let Ok(n) = v.parse::<f32>() {
            return format_f32(n);
        }
        let snake = crate::naming::to_snake_case(v);
        let in_style = self
            .constants
            .iter()
            .any(|c| crate::naming::to_snake_case(&c.name) == snake);
        let looks_like_color_name = is_simple_ident(v)
            && v.chars()
                .next()
                .is_some_and(|c| c.is_ascii_lowercase() || c == '_');
        if in_style || (self.theme_type.is_some() && looks_like_color_name) {
            return self.color_expr(v);
        }
        v.to_string()
    }

    fn emit_widget_ref(&mut self, el: &Element) -> ChildEmit {
        let var = el.content.as_deref().unwrap_or("").trim().to_string();
        ChildEmit::Simple {
            name: var,
            code: String::new(),
        }
    }

    /// Builds the `LayoutStyle` expression for a container: base style from the
    /// tag (or a class function), then inline attribute modifiers chained on.
    fn make_layout_style(&self, tag: &str, classes: &[String], attrs: &[Attr]) -> String {
        let mut expr = if let Some(first) = classes.first() {
            // The first class provides the base style; further classes cannot
            // currently compose, so only the first is applied.
            let mut base = format!("{}()", style_fn_name(first));
            // Apply the tag's flex direction only when no class declares one,
            // so a styled `row .card` still lays out horizontally.
            if !self.class_sets_direction(first) {
                match tag {
                    "row" | "grid" => base.push_str(".flex_row()"),
                    "col" => base.push_str(".flex_column()"),
                    _ => {}
                }
            }
            base
        } else {
            match tag {
                "row" => "LayoutStyle::new().flex_row()".to_string(),
                "col" | "box" => "LayoutStyle::new().flex_column()".to_string(),
                // `cols:` adds `.display_grid()`, so start neutral; fall back to flex_row
                // when no cols are declared (legacy behaviour).
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

    /// Whether the named class declares a flex `direction`, so the tag should
    /// not override it.
    fn class_sets_direction(&self, class_name: &str) -> bool {
        self.classes
            .iter()
            .find(|c| c.name == class_name)
            .map(|c| c.props.iter().any(|p| p.key == "direction"))
            .unwrap_or(false)
    }

    // ----- Control flow -----------------------------------------------------

    fn emit_if(&mut self, block: &IfBlock) -> ChildEmit {
        let pad = self.pad();
        let mut code = String::new();
        let _ = writeln!(code, "{pad}if {} {{", block.condition.trim());
        self.indent += 1;
        self.emit_branch_into_children(&block.then_branch, &mut code);
        self.indent -= 1;

        if let Some(else_branch) = &block.else_branch {
            let _ = writeln!(code, "{pad}}} else {{");
            self.indent += 1;
            self.emit_branch_into_children(else_branch, &mut code);
            self.indent -= 1;
        }
        let _ = write!(code, "{pad}}}");
        ChildEmit::Dynamic { code }
    }

    fn emit_for(&mut self, block: &ForBlock) -> ChildEmit {
        let pad = self.pad();
        let mut code = String::new();
        let _ = writeln!(
            code,
            "{pad}for {} in {} {{",
            block.pattern.trim(),
            block.iterable.trim()
        );
        self.indent += 1;
        // Loop variables are often borrowed (`items.iter()`), but widget closures
        // require `'static` captures; bind owned copies so they can be moved in.
        let body_pad = self.pad();
        let idents = pattern_idents(&block.pattern);
        for ident in &idents {
            let _ = writeln!(code, "{body_pad}let {ident} = {ident}.to_owned();");
        }
        let added = idents.len();
        self.loop_vars.extend(idents);
        self.emit_branch_into_children(&block.body, &mut code);
        self.loop_vars.truncate(self.loop_vars.len() - added);
        self.indent -= 1;
        let _ = write!(code, "{pad}}}");
        ChildEmit::Dynamic { code }
    }

    /// Emits a control-flow branch's nodes, pushing every produced widget into
    /// the surrounding `__children` vector.
    fn emit_branch_into_children(&mut self, nodes: &[ViewNode], code: &mut String) {
        let pad = self.pad();
        for node in nodes {
            match self.emit_node(node) {
                ChildEmit::Simple { name, code: c } => {
                    let _ = writeln!(code, "{c}");
                    let _ = writeln!(
                        code,
                        "{pad}__children.push(Box::new({name}) as Box<dyn LayoutItem>);"
                    );
                }
                ChildEmit::Dynamic { code: c } => {
                    let _ = writeln!(code, "{c}");
                }
            }
        }
    }

    // ----- Content / color helpers -----------------------------------------

    /// Builds the `content_fn` closure for a text node, handling `{...}` interpolation.
    pub fn interpolate_content(&self, content: &str) -> String {
        let segments = parse_interpolation(content);
        if segments.iter().all(|s| matches!(s, Segment::Literal(_))) {
            return format!("|| {}.to_string()", rust_str(content));
        }

        let mut fmt = String::new();
        let mut args = Vec::new();
        for seg in &segments {
            match seg {
                Segment::Literal(text) => {
                    fmt.push_str(&text.replace('{', "{{").replace('}', "}}"));
                }
                Segment::Expr(expr) => {
                    fmt.push_str("{}");
                    args.push(self.render_interp_expr(expr));
                }
            }
        }

        let args_joined = args.join(", ");
        format!("move || format!({}, {args_joined})", rust_str(&fmt))
    }

    /// Renders an interpolation expression: a bare signal name becomes `name.get()`,
    /// anything else is emitted as a braced Rust expression.
    fn render_interp_expr(&self, expr: &str) -> String {
        let trimmed = expr.trim();
        if self.signals.iter().any(|s| s.name == trimmed) {
            format!("{trimmed}.get()")
        } else if is_simple_ident(trimmed) {
            trimmed.to_string()
        } else {
            format!("{{ {trimmed} }}")
        }
    }

    /// Resolves a color reference: an inline hex value, a CSS keyword, a
    /// `Color::*` literal, a `[style]`-declared local constant, or a theme field.
    ///
    /// Lookup order:
    /// 1. Inline hex / `Color::*` / CSS keyword → static expression.
    /// 2. Declared in `[style]` → file-local `COLOR_*` constant (non-reactive).
    /// 3. Not declared + `theme_type` set → `use_theme::<T>().field` (reactive).
    /// 4. Not declared + no theme → `COLOR_*` (rustc catches the missing symbol).
    ///
    /// Declaring a color in `[style]` therefore acts as a local override that
    /// takes precedence over the theme, which is the common case for one-off
    /// palette values that should not track theme switches.
    fn color_expr(&self, value: &str) -> String {
        let v = value.trim();
        if v.starts_with('#') {
            return hex_to_color_expr(v);
        }
        if v.starts_with("Color::") {
            return v.to_string();
        }
        match v {
            "white" => return "Color::WHITE".to_string(),
            "black" => return "Color::BLACK".to_string(),
            "transparent" => return "Color::TRANSPARENT".to_string(),
            _ => {}
        }
        let snake = to_snake_case(v);
        if self
            .constants
            .iter()
            .any(|c| to_snake_case(&c.name) == snake)
        {
            return const_name("COLOR_", v);
        }
        if let Some(theme) = &self.theme_type {
            return format!("use_theme::<{theme}>().{snake}");
        }
        const_name("COLOR_", v)
    }

    /// Whether codegen resolves any color through `use_theme`, requiring the import.
    pub fn uses_theme(&self) -> bool {
        self.theme_type.is_some()
    }
}

// ----- Free helpers ---------------------------------------------------------

enum Segment {
    Literal(String),
    Expr(String),
}

/// Splits a string into literal and `{expr}` segments. Escaped braces `{{`/`}}`
/// are treated as literal single braces.
fn parse_interpolation(content: &str) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut literal = String::new();
    let mut chars = content.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                literal.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                literal.push('}');
            }
            '{' => {
                if !literal.is_empty() {
                    segments.push(Segment::Literal(std::mem::take(&mut literal)));
                }
                let mut expr = String::new();
                for ec in chars.by_ref() {
                    if ec == '}' {
                        break;
                    }
                    expr.push(ec);
                }
                segments.push(Segment::Expr(expr));
            }
            _ => literal.push(c),
        }
    }
    if !literal.is_empty() {
        segments.push(Segment::Literal(literal));
    }
    segments
}

/// Extracts the binding identifiers from a `for` pattern, ignoring tuple
/// punctuation and the `_` wildcard. `(i, item)` -> `["i", "item"]`.
fn pattern_idents(pattern: &str) -> Vec<String> {
    let mut idents = Vec::new();
    let mut current = String::new();
    for c in pattern.chars() {
        if c == '_' || c.is_ascii_alphanumeric() {
            current.push(c);
        } else if !current.is_empty() {
            idents.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        idents.push(current);
    }
    idents
        .into_iter()
        .filter(|i| {
            i != "_"
                && i.chars()
                    .next()
                    .is_some_and(|c| c == '_' || c.is_ascii_alphabetic())
        })
        .collect()
}

fn is_simple_ident(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c == '_' || c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

/// Renders a Rust string literal, escaping quotes and backslashes.
fn rust_str(s: &str) -> String {
    let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

/// The parser strips `on_press:` leaving `|| expr` or `|ev| expr`. Ensure the
/// value is a closure; wrap bare expressions in a zero-arg closure.
fn normalize_closure(value: &str) -> String {
    let v = value.trim();
    if v.starts_with('|') {
        v.to_string()
    } else {
        format!("|| {{ {v} }}")
    }
}

/// Assembles a `&[(pos, color)]` gradient stops expression from the resolved
/// `from`, `to`, and optional `mid`/`mid_pos` values.
fn build_gradient_stops(from: &str, to: &str, mid: Option<&str>, mid_pos: f32) -> String {
    if let Some(m) = mid {
        format!(
            "&[(0.0, {from}), ({}, {m}), (1.0, {to})]",
            format_f32(mid_pos)
        )
    } else {
        format!("&[(0.0, {from}), (1.0, {to})]")
    }
}

/// Builds a `RectStyle { … }` or shorthand expression from the resolved fill,
/// stroke, shadow, and radius values. Mirrors the branching logic shared by
/// `emit_box` and `emit_canvas_rect`.
fn build_rect_style(
    gradient: Option<String>,
    solid_fill: Option<String>,
    stroke: Option<String>,
    stroke_w: f32,
    shadow: Option<String>,
    radius: &str,
) -> String {
    if shadow.is_some() || stroke.is_some() || gradient.is_some() {
        let fill_s = gradient
            .map(|g| format!("Some({g})"))
            .or_else(|| solid_fill.map(|f| format!("Some(Paint::Solid({f}))")))
            .unwrap_or_else(|| "None".to_string());
        let stroke_s = stroke
            .map(|s| format!("Some(Stroke::new({s}, {}))", format_f32(stroke_w)))
            .unwrap_or_else(|| "None".to_string());
        let shadow_s = shadow.unwrap_or_else(|| "None".to_string());
        format!(
            "RectStyle {{ fill: {fill_s}, stroke: {stroke_s}, shadow: {shadow_s}, radius: {radius} }}"
        )
    } else {
        match solid_fill {
            Some(f) => format!("RectStyle::filled({f}, {radius})"),
            None => "RectStyle::default()".to_string(),
        }
    }
}

/// Binds canvas closure params (`w, h`) to fields of the `Rect` argument.
fn canvas_param_bindings(params: &str, pad: &str) -> String {
    let mut out = String::new();
    let names: Vec<&str> = params
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    // Convention: first param is width, second is height.
    if let Some(w) = names.first() {
        let _ = writeln!(out, "{pad}    let {w} = __rect.width;");
    }
    if let Some(h) = names.get(1) {
        let _ = writeln!(out, "{pad}    let {h} = __rect.height;");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signal_scan::{SignalInfo, SignalKind};

    fn make_gen<'a>(signals: &'a [SignalInfo]) -> ViewGen<'a> {
        ViewGen::with_theme(signals, &[], &[], None)
    }

    #[test]
    fn literal_content() {
        let g = make_gen(&[]);
        assert_eq!(g.interpolate_content("hello"), "|| \"hello\".to_string()");
    }

    #[test]
    fn signal_interpolation() {
        let signals = vec![SignalInfo {
            name: "count".into(),
            kind: SignalKind::RwSignal,
        }];
        let g = make_gen(&signals);
        assert_eq!(
            g.interpolate_content("Count: {count}"),
            "move || format!(\"Count: {}\", count.get())"
        );
    }

    #[test]
    fn closure_passthrough() {
        assert_eq!(
            normalize_closure("|| count.update(|n| *n += 1)"),
            "|| count.update(|n| *n += 1)"
        );
    }

    #[test]
    fn widget_ref_passthrough() {
        let src = "[logic]\nlet canvas = build_canvas(ctx)?;\n[view]\nwidget \"canvas\"\n";
        let out = crate::transpile_source_with_theme(src, "my_section", None).unwrap();
        assert!(out.rust_code.contains("Ok(Box::new(canvas))"));
    }

    #[test]
    fn canvas_with_rect_and_text_children() {
        let src = "[logic]\n[view]\ncanvas width:100 height:50\n    rect fill:#3c77fa radius:8\n    text \"hi\" x:0 y:4 w:full h:42 size:12 color:white\n";
        let out = crate::transpile_source_with_theme(src, "demo", None).unwrap();
        let code = &out.rust_code;
        assert!(code.contains("Canvas::new(ctx,"), "missing Canvas::new");
        assert!(
            code.contains("let __w = __rect.width;"),
            "missing __w binding"
        );
        assert!(
            code.contains("let __h = __rect.height;"),
            "missing __h binding"
        );
        assert!(
            code.contains("RenderNode::rect("),
            "missing RenderNode::rect"
        );
        assert!(
            code.contains("RenderNode::group(["),
            "missing RenderNode::group"
        );
        assert!(
            code.contains("RenderNode::text("),
            "missing RenderNode::text"
        );
        assert!(code.contains("__w"), "w:full should resolve to __w");
        assert!(
            code.contains("Color::WHITE"),
            "color:white should resolve to Color::WHITE"
        );
    }

    #[test]
    fn unknown_tag_becomes_component_call() {
        let src = "[logic]\n[view]\nmy_card\n";
        let out = crate::transpile_source_with_theme(src, "demo", None).unwrap();
        assert!(
            out.rust_code.contains("my_card(ctx)?"),
            "no-attr tag should call fn directly"
        );
    }
}
