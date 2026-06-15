//! Generates the body of the component function from the `[view]` section.

use std::fmt::Write;

use rsx_parser::{Attr, Element, ForBlock, IfBlock, StyleClass, StyleConst, ViewNode};

use crate::naming::{const_name, mentions_ident, style_fn_name, to_snake_case};
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
    counter: usize,
    /// Indentation depth (in 4-space units) for the current emission scope.
    indent: usize,
    /// Loop-variable identifiers currently in scope, cloned per closure like signals.
    loop_vars: Vec<String>,
}

impl<'a> ViewGen<'a> {
    pub fn new(
        signals: &'a [SignalInfo],
        classes: &'a [StyleClass],
        constants: &'a [StyleConst],
    ) -> Self {
        Self {
            signals,
            classes,
            constants,
            counter: 0,
            indent: 1,
            loop_vars: Vec::new(),
        }
    }

    fn next_var(&mut self) -> String {
        let v = format!("__w{}", self.counter);
        self.counter += 1;
        v
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
            "scroll" => self.emit_scroll(el),
            "canvas" => self.emit_canvas(el),
            other => ChildEmit::Dynamic {
                code: format!("{}// TODO: unsupported tag `{other}`", self.pad()),
            },
        }
    }

    // ----- Leaf widgets -----------------------------------------------------

    fn emit_text(&mut self, el: &Element) -> ChildEmit {
        let var = self.next_var();
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
        let line_height = format_f32(font_size * 1.4);
        let layout_style = format!("LayoutStyle::new().height({line_height})");

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

    fn emit_button(&mut self, el: &Element) -> ChildEmit {
        let var = self.next_var();
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
        let var = self.next_var();
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

    fn emit_scroll(&mut self, el: &Element) -> ChildEmit {
        let var = self.next_var();
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
                "{inner_pad}ScrollArea::as_layout_item(ctx, {style}, Box::new(__scroll_content))?"
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
                "{inner_pad}ScrollArea::as_layout_item(ctx, {style}, Box::new({content}))?"
            );
        }

        let _ = write!(code, "{pad}}};");
        ChildEmit::Simple { name: var, code }
    }

    fn emit_canvas(&mut self, el: &Element) -> ChildEmit {
        let var = self.next_var();
        let pad = self.pad();
        let style = self.make_layout_style(&el.tag, &el.classes, &el.attrs);

        // Canvas children render to RenderNode rather than LayoutItem; full
        // drawing codegen is out of MVP scope, so emit a stub draw function.
        let params = el.canvas_params.as_deref().unwrap_or("");
        let bindings = canvas_param_bindings(params, &pad);

        let mut code = String::new();
        let _ = writeln!(
            code,
            "{pad}let {var} = Canvas::new(ctx, {style}, move |__rect| {{"
        );
        code.push_str(&bindings);
        let _ = writeln!(code, "{pad}    RenderNode::group([])");
        let _ = write!(code, "{pad}}})?;");
        ChildEmit::Simple { name: var, code }
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
                "col" => "LayoutStyle::new().flex_column()".to_string(),
                // grid is not implemented; fall back to a row.
                "grid" => "LayoutStyle::new().flex_row()".to_string(),
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

    /// Resolves a color reference: a known constant name, an inline hex value,
    /// or a `Color::*` literal passed through.
    fn color_expr(&self, value: &str) -> String {
        let v = value.trim();
        if v.starts_with('#') {
            return hex_to_color_expr(v);
        }
        if v.starts_with("Color::") {
            return v.to_string();
        }
        let snake = to_snake_case(v);
        if self
            .constants
            .iter()
            .any(|c| to_snake_case(&c.name) == snake)
        {
            return const_name("COLOR_", v);
        }
        // Unknown reference; fall back to the screaming-snake constant name so
        // the user gets a clear missing-symbol error rather than silent wrong output.
        const_name("COLOR_", v)
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
        ViewGen::new(signals, &[], &[])
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
}
