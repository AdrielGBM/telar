//! Generates the body of the component function from the `[view]` section.

use std::collections::HashMap;
use std::fmt::Write;

use rsx_parser::{Attr, Element, ForBlock, IfBlock, StyleClass, StyleConstant, ViewNode};

use crate::naming::{
    constant_name, contains_ident, is_ident, style_function_name, to_pascal_case, to_snake_case,
};
use crate::style::{format_f32, hex_to_color_expr, layout_prop_call};

// `heading`/`section` styling reproduced inline (the library no longer ships `Heading`/`Section`): a 12px caption colored from the theme's `widget_muted` token, and an 8px-gap column wrapping the heading above its content.
const HEADING_FONT_SIZE: &str = "12.0";
const SECTION_GAP: &str = "8.0";
const HEADING_STYLE_CLOSURE: &str = "move || { let color = use_widget_theme().map(|t| t.widget_muted()).unwrap_or(Color::rgba(0.5, 0.5, 0.6, 1.0)); TextStyle::new(12.0, color) }";

/// Sentinel comment lines that bracket each view node's generated code with the `.rsx` line it came from. They are emitted into the view body during generation and stripped by [`resolve_source_map`] in the transpiler, which turns them into the per-line origin map. The prefix is deliberately un-generatable by normal codegen so it can never collide with real output.
const SRC_PUSH: &str = "//@RSX@PUSH:";
const SRC_POP: &str = "//@RSX@POP";

/// Inline marker emitted immediately before a verbatim `[view]` Rust expression (interpolation `{expr}`, `if`/`let` expressions, closure attr values). Stripped by [`resolve_source_map`], which records the byte position where the expression begins in the generated body, so the analyzer can map a `.rsx` cursor inside the expression onto the generated Rust. Payload: `<rsx_start>:<len>`, the source byte offset and length of the fragment, which is byte-identical in source and output.
const SRC_EXPR_OPEN: &str = "/*@RSX@EXPR:";
const SRC_EXPR_CLOSE: &str = "@*/";

/// Builds an [`SRC_EXPR_OPEN`] marker for a verbatim expression at source byte offset `rsx_start` spanning `len` bytes.
fn expr_marker(rsx_start: usize, len: usize) -> String {
    format!("{SRC_EXPR_OPEN}{rsx_start}:{len}{SRC_EXPR_CLOSE}")
}

/// Wraps `emit`'s code in `SRC_PUSH`/`SRC_POP` markers carrying the 0-based `.rsx` `line`.
fn wrap_source_markers(emit: ChildEmit, line: usize) -> ChildEmit {
    let src0 = line.saturating_sub(1);
    let wrap = |code: String| format!("{SRC_PUSH}{src0}\n{code}\n{SRC_POP}");
    match emit {
        ChildEmit::Simple { name, code } => ChildEmit::Simple {
            name,
            code: wrap(code),
        },
        ChildEmit::Dynamic { code } => ChildEmit::Dynamic { code: wrap(code) },
    }
}

/// The resolved view body: real lines (each with its origin `.rsx` line) plus the byte spans of the verbatim Rust expressions it contains.
pub(crate) struct ResolvedView {
    pub lines: Vec<(String, Option<u32>)>,
    /// Per expression: `(byte offset within the streamed body, rsx_start, len)`. The streamed body is the lines joined with `\n` (each line followed by a newline), matching how `transpile` appends them, so a caller adds the body's start offset in the final file to get the generated offset.
    pub expr_spans: Vec<(usize, u32, u32)>,
}

/// Strips the source markers from a generated view body, returning each real line paired with the `.rsx` line it originated from (a stack tracks nesting, so a node's own lines map to itself and its children's lines map to the children) plus the verbatim-expression byte spans. Lines outside any marker (root boilerplate) map to `None`.
pub(crate) fn resolve_source_map(marked: &str) -> ResolvedView {
    let mut stack: Vec<u32> = Vec::new();
    let mut lines = Vec::new();
    let mut expr_spans = Vec::new();
    // Running byte length of the streamed body (each emitted line plus its trailing `\n`).
    let mut body_len = 0usize;
    for line in marked.split('\n') {
        if let Some(rest) = line.strip_prefix(SRC_PUSH) {
            if let Ok(n) = rest.parse::<u32>() {
                stack.push(n);
            }
        } else if line == SRC_POP {
            stack.pop();
        } else {
            let (clean, spans) = strip_expr_markers(line, body_len);
            expr_spans.extend(spans);
            body_len += clean.len() + 1;
            lines.push((clean, stack.last().copied()));
        }
    }
    ResolvedView { lines, expr_spans }
}

/// Removes inline [`SRC_EXPR_OPEN`] markers from a single output `line`, returning the cleaned line and, for each marker, `(body offset of the following expression, rsx_start, len)`. `base` is the body byte offset of this line's start; the expression begins exactly where the marker was, so its offset is `base + <cleaned bytes emitted so far>`.
fn strip_expr_markers(line: &str, base: usize) -> (String, Vec<(usize, u32, u32)>) {
    let mut out = String::with_capacity(line.len());
    let mut spans = Vec::new();
    let mut rest = line;
    while let Some(open) = rest.find(SRC_EXPR_OPEN) {
        out.push_str(&rest[..open]);
        let after_open = &rest[open + SRC_EXPR_OPEN.len()..];
        let Some(close) = after_open.find(SRC_EXPR_CLOSE) else {
            // Malformed marker (no close): keep the remainder verbatim and stop.
            out.push_str(rest);
            return (out, spans);
        };
        let payload = &after_open[..close];
        if let Some((rsx_start, len)) = payload.split_once(':')
            && let (Ok(rsx_start), Ok(len)) = (rsx_start.parse::<u32>(), len.parse::<u32>())
        {
            spans.push((base + out.len(), rsx_start, len));
        }
        rest = &after_open[close + SRC_EXPR_CLOSE.len()..];
    }
    out.push_str(rest);
    (out, spans)
}

/// A piece of generated child code together with how it contributes to a parent's child collection.
enum ChildEmit {
    /// A simple widget bound to `name`, pushable directly.
    Simple { name: String, code: String },
    /// Control flow (`if`/`for`) that mutates a child vector in place.
    Dynamic { code: String },
}

pub struct ViewGen<'a> {
    /// Declared style classes, used to validate class references in elements.
    classes: &'a [StyleClass],
    constants: &'a [StyleConstant],
    /// Per-widget-type variable counters, keyed by the descriptive prefix.
    counters: HashMap<String, usize>,
    /// When set, `[style]` color references resolve to `use_theme::<Type>().field` instead of generated `COLOR_*` consts, so theme switching takes effect.
    theme_type: Option<String>,
    /// Indentation depth (in 4-space units) for the current emission scope.
    indent: usize,
    /// Loop-variable identifiers currently in scope, cloned per closure like signals.
    loop_variables: Vec<String>,
}

impl<'a> ViewGen<'a> {
    pub fn with_theme(
        classes: &'a [StyleClass],
        constants: &'a [StyleConstant],
        theme_type: Option<&str>,
    ) -> Self {
        Self {
            classes,
            constants,
            counters: HashMap::new(),
            theme_type: theme_type.map(str::to_string),
            indent: 1,
            loop_variables: Vec::new(),
        }
    }

    fn next_variable_name(&mut self, tag: &str) -> String {
        let prefix = match tag {
            "text" => "text",
            "btn" | "button" => "btn",
            "col" | "column" => "col",
            "row" => "row",
            "box" => "sbox",
            "img" | "image" => "img",
            "svg" => "svg",
            "canvas" => "canvas",
            _ => "node",
        };
        let count = self.counters.entry(prefix.to_string()).or_insert(0);
        let name = format!("__{prefix}_{count}");
        *count += 1;
        name
    }

    fn indent_str(&self) -> String {
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
                    // A bare control-flow node at the root has no container to attach to; emit it verbatim for completeness.
                    out.push_str(&code);
                    out.push('\n');
                }
            }
        }

        let pad = self.indent_str();
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
        let emit = match node {
            ViewNode::Element(el) => self.emit_element(el),
            ViewNode::LetStmt {
                source,
                source_start,
            } => ChildEmit::Dynamic {
                code: format!(
                    "{}{}{source};",
                    self.indent_str(),
                    expr_marker(*source_start, source.len())
                ),
            },
            ViewNode::IfBlock(block) => self.emit_if(block),
            ViewNode::ForBlock(block) => self.emit_for(block),
        };
        // Bracket this node's generated lines with source markers so the transpiler can map them back to the `.rsx` line. Nested nodes nest their own markers; `let` statements have no line of their own and inherit the enclosing node's mapping.
        match node {
            ViewNode::Element(el) => wrap_source_markers(emit, el.line),
            ViewNode::IfBlock(block) => wrap_source_markers(emit, block.line),
            ViewNode::ForBlock(block) => wrap_source_markers(emit, block.line),
            ViewNode::LetStmt { .. } => emit,
        }
    }

    fn emit_element(&mut self, el: &Element) -> ChildEmit {
        match el.tag.as_str() {
            "text" => self.emit_text(el),
            "heading" => self.emit_heading(el),
            "section" => self.emit_section(el),
            "btn" => self.emit_button(el),
            "col" | "row" | "grid" => self.emit_container(el),
            "box" => self.emit_box(el),
            "img" | "image" => self.emit_image(el),
            "svg" => self.emit_svg(el),
            "scroll" => self.emit_scroll(el),
            "canvas" => self.emit_canvas(el),
            "widget" => self.emit_widget_ref(el),
            other => self.emit_component_call(el, other),
        }
    }

    fn emit_text(&mut self, el: &Element) -> ChildEmit {
        let var = self.next_variable_name(&el.tag);
        let pad = self.indent_str();
        let content = el.content.as_deref().unwrap_or("");
        let content_fn = self.interpolate_content(content, el.content_start);
        let style = self.text_style(&el.attributes);

        let mut extra = String::new();
        for a in &el.attributes {
            if matches!(a.key.as_str(), "size" | "color" | "lines" | "height") {
                continue;
            }
            if let Some(call) = layout_prop_call(&a.key, &a.value) {
                extra.push_str(&call);
            }
        }
        // An explicit `height:` pins the box; otherwise the leaf measures its own height from the wrapped content (`Text::auto`) so multi-line text reserves real space and pushes following siblings down instead of overflowing.
        let explicit_height = el
            .attributes
            .iter()
            .find(|a| a.key == "height")
            .and_then(|a| layout_prop_call("height", &a.value));

        let (ctor, layout_style) = match explicit_height {
            Some(h) => ("Text::new", format!("LayoutStyle::new(){h}{extra}")),
            None => ("Text::auto", format!("LayoutStyle::new(){extra}")),
        };

        // Each `move` closure consumes its captures; clone the signals they use into block locals so both closures can capture independently. Scan the raw `content` (still carrying `$`), not the substituted `content_fn`.
        let clones = self.clone_bindings(&[content, style.as_str()], &pad, "    ");

        let code = format!(
            "{pad}let {var} = {{\n\
             {clones}\
             {pad}    {ctor}(\n\
             {pad}        ctx,\n\
             {pad}        {content_fn},\n\
             {pad}        {layout_style},\n\
             {pad}        {style},\n\
             {pad}    )?\n\
             {pad}}};"
        );
        ChildEmit::Simple { name: var, code }
    }

    // A muted section caption: a single-line `Text` whose color reads `widget_muted` from the active theme (falling back to a neutral gray when none is set).
    fn emit_heading(&mut self, el: &Element) -> ChildEmit {
        let var = self.next_variable_name("heading");
        let pad = self.indent_str();
        let content = el.content.as_deref().unwrap_or("");
        let content_fn = self.interpolate_content(content, el.content_start);
        let clones = self.clone_bindings(&[content], &pad, "    ");
        let style_closure = HEADING_STYLE_CLOSURE;
        let code = format!(
            "{pad}let {var} = {{\n\
             {clones}\
             {pad}    Text::new(\n\
             {pad}        ctx,\n\
             {pad}        {content_fn},\n\
             {pad}        LayoutStyle::new().height({HEADING_FONT_SIZE}_f32 * 1.4),\n\
             {pad}        {style_closure},\n\
             {pad}    )?\n\
             {pad}}};"
        );
        ChildEmit::Simple { name: var, code }
    }

    // A titled column: a muted `heading` Text prepended to the children inside a `flex_column` Container with a small gap. Child emission mirrors `emit_container`; the title comes from `content` rather than a child node.
    fn emit_section(&mut self, el: &Element) -> ChildEmit {
        let var = self.next_variable_name("section");
        let heading_var = self.next_variable_name("heading");
        let pad = self.indent_str();
        let content = el.content.as_deref().unwrap_or("");
        let title_fn = self.interpolate_content(content, el.content_start);
        let style_closure = HEADING_STYLE_CLOSURE;

        let has_dynamic = el.children.iter().any(|n| {
            matches!(
                n,
                ViewNode::IfBlock(_) | ViewNode::ForBlock(_) | ViewNode::LetStmt { .. }
            )
        });

        self.indent += 1;
        let inner_pad = self.indent_str();
        let mut child_emits = Vec::new();
        for child in &el.children {
            child_emits.push(self.emit_node(child));
        }
        self.indent -= 1;

        // Clone signals captured by the title closure so children can still use them (scan raw `content`).
        let clones = self.clone_bindings(&[content], &inner_pad, "");

        let mut code = String::new();
        let _ = writeln!(code, "{pad}let {var} = {{");
        let _ = write!(code, "{clones}");
        let _ = writeln!(
            code,
            "{inner_pad}let {heading_var} = Text::new(ctx, {title_fn}, LayoutStyle::new().height({HEADING_FONT_SIZE}_f32 * 1.4), {style_closure})?;"
        );

        let children = self.emit_children_collection(
            &mut code,
            &child_emits,
            &inner_pad,
            has_dynamic,
            std::slice::from_ref(&heading_var),
        );
        let _ = writeln!(
            code,
            "{inner_pad}Container::new(ctx, LayoutStyle::new().flex_column().gap({SECTION_GAP}), {children})?"
        );

        let _ = write!(code, "{pad}}};");
        ChildEmit::Simple { name: var, code }
    }

    /// Emits the children of a container-like element into `code` and returns the expression to pass as the constructor's children argument. `seed` names are prepended before the emitted children (e.g. a `section`'s heading). When any child is dynamic control flow, this builds a mutable `__children` vec and returns `__children`; otherwise it returns a `children![...]` literal. Used by `emit_section`/`emit_container`/`emit_box`; `emit_scroll` differs and is intentionally excluded.
    fn emit_children_collection(
        &self,
        code: &mut String,
        child_emits: &[ChildEmit],
        inner_pad: &str,
        has_dynamic: bool,
        seed: &[String],
    ) -> String {
        if has_dynamic {
            let _ = writeln!(
                code,
                "{inner_pad}let mut __children: Vec<Box<dyn LayoutItem>> = Vec::new();"
            );
            for name in seed {
                let _ = writeln!(code, "{inner_pad}__children.push(box_item({name}));");
            }
            for emit in child_emits {
                match emit {
                    ChildEmit::Simple { name, code: c } => {
                        let _ = writeln!(code, "{c}");
                        let _ = writeln!(code, "{inner_pad}__children.push(box_item({name}));");
                    }
                    ChildEmit::Dynamic { code: c } => {
                        let _ = writeln!(code, "{c}");
                    }
                }
            }
            "__children".to_string()
        } else {
            let mut names: Vec<String> = seed.to_vec();
            for emit in child_emits {
                match emit {
                    ChildEmit::Simple { name, code: c } => {
                        let _ = writeln!(code, "{c}");
                        names.push(name.clone());
                    }
                    ChildEmit::Dynamic { code: c } => {
                        let _ = writeln!(code, "{c}");
                    }
                }
            }
            format!("children![{}]", names.join(", "))
        }
    }

    /// Emits `let name = name.clone();` for every signal (`$name`) referenced in the *raw* `snippets` — still carrying the `$` sigil, so captures are detected before substitution — plus any loop variable in scope they use. Indented under `pad + extra`.
    fn clone_bindings(&self, snippets: &[&str], pad: &str, extra: &str) -> String {
        let mut used: Vec<String> = Vec::new();
        for s in snippets {
            for ident in signal_idents(s) {
                if !used.contains(&ident) {
                    used.push(ident);
                }
            }
        }
        for var in &self.loop_variables {
            if snippets.iter().any(|s| contains_ident(s, var)) && !used.contains(var) {
                used.push(var.clone());
            }
        }
        let mut out = String::new();
        for name in &used {
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
        let var = self.next_variable_name("img");
        let pad = self.indent_str();

        // `src` is a verbatim Rust expression (e.g. `gradient_img`). Tag it with its source span so the analyzer can resolve / rename the symbol inside it; quoted values keep the legacy passthrough.
        let src = match el.attributes.iter().find(|a| a.key == "src") {
            Some(a) if !a.is_quoted && !a.value.trim().is_empty() => {
                let v = a.value.trim();
                let lead = a.value.len() - a.value.trim_start().len();
                format!("{}{v}", expr_marker(a.value_start + lead, v.len()))
            }
            Some(a) => a.value.clone(),
            None => "__img_data".to_string(),
        };

        let filter = el
            .attributes
            .iter()
            .find(|a| a.key == "filter")
            .map(|a| match a.value.trim() {
                "Nearest" | "nearest" => "ImageFilter::Nearest",
                _ => "ImageFilter::Linear",
            })
            .unwrap_or("ImageFilter::Linear");

        let layout_style = self.make_layout_style("img", &el.classes, &el.attributes);

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

    /// Mirrors `emit_image`: `src` is a verbatim `Arc<SvgData>` expression, hoisted once into `__src` so the reactive closure only clones the (cheap) Arc handle. `tint` is optional and, unlike `src`, is embedded directly in its closure since a `Color` is cheap to recompute per call.
    fn emit_svg(&mut self, el: &Element) -> ChildEmit {
        let var = self.next_variable_name("svg");
        let pad = self.indent_str();

        // Same verbatim-with-span handling as `img`'s `src`; a missing attr falls back to an undefined identifier so rustc's error lands on this `.rsx` line via the source map.
        let src = match el.attributes.iter().find(|a| a.key == "src") {
            Some(a) if !a.is_quoted && !a.value.trim().is_empty() => {
                let v = a.value.trim();
                let lead = a.value.len() - a.value.trim_start().len();
                format!("{}{v}", expr_marker(a.value_start + lead, v.len()))
            }
            Some(a) => a.value.clone(),
            None => "__svg_data".to_string(),
        };

        let tint = el.attributes.iter().find(|a| a.key == "tint").map(|a| {
            if !a.is_quoted && !a.value.trim().is_empty() {
                let v = a.value.trim();
                let lead = a.value.len() - a.value.trim_start().len();
                format!("{}{v}", expr_marker(a.value_start + lead, v.len()))
            } else {
                a.value.clone()
            }
        });
        let tint_fn = match tint {
            Some(expr) => format!("move || Some({expr})"),
            None => "|| None".to_string(),
        };

        let layout_style = self.make_layout_style("svg", &el.classes, &el.attributes);

        let code = format!(
            "{pad}let {var} = {{\n\
             {pad}    let __src = {src}.clone();\n\
             {pad}    Svg::new(\n\
             {pad}        ctx,\n\
             {pad}        {layout_style},\n\
             {pad}        move || __src.clone(),\n\
             {pad}        {tint_fn},\n\
             {pad}    )?\n\
             {pad}}};"
        );

        ChildEmit::Simple { name: var, code }
    }

    fn emit_button(&mut self, el: &Element) -> ChildEmit {
        let var = self.next_variable_name(&el.tag);
        let pad = self.indent_str();
        let label = el.content.as_deref().unwrap_or("");
        let style = self.button_style(&el.attributes, pad.as_str());

        let on_press_attr = el.attributes.iter().find(|a| a.key == "on_press");
        let on_press = on_press_attr.map(|h| normalize_closure(&h.value));

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
        if let Some(raw_closure) = on_press {
            let closure = substitute_handles(&raw_closure);
            // A verbatim span maps only when the closure is copied byte-for-byte; a `$` substitution (like `normalize_closure` rewriting a bare expression) breaks that, so it gets no marker.
            let marker = if raw_closure.contains('$') {
                String::new()
            } else {
                closure_marker(on_press_attr)
            };
            let _ = writeln!(
                code,
                "{pad}    __btn = __btn.on_click(move {marker}{closure});"
            );
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

    fn emit_container(&mut self, el: &Element) -> ChildEmit {
        let var = self.next_variable_name(&el.tag);
        let pad = self.indent_str();
        let style = self.make_layout_style(&el.tag, &el.classes, &el.attributes);

        // A `col`/`row` with paint (inline or from its class) upgrades to a StyledContainer so it can carry a background like `box`; otherwise it stays a plain Container.
        let pattrs = self.paint_attrs(el);
        let pieces = has_paint(&pattrs).then(|| self.rect_style_pieces(&pattrs));

        let has_dynamic = el.children.iter().any(|n| {
            matches!(
                n,
                ViewNode::IfBlock(_) | ViewNode::ForBlock(_) | ViewNode::LetStmt { .. }
            )
        });

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
        match pieces {
            Some((param, rect_style, opacity_call)) => {
                let _ = writeln!(
                    code,
                    "{inner_pad}StyledContainer::new(ctx, {style}, move |{param}| {rect_style}, {children})?{opacity_call}"
                );
            }
            None => {
                let _ = writeln!(code, "{inner_pad}Container::new(ctx, {style}, {children})?");
            }
        }

        let _ = write!(code, "{pad}}};");
        ChildEmit::Simple { name: var, code }
    }

    fn emit_box(&mut self, el: &Element) -> ChildEmit {
        let var = self.next_variable_name("box");
        let pad = self.indent_str();
        let layout_style = self.make_layout_style("box", &el.classes, &el.attributes);

        // Paint merges inline attrs with the element's class (inline wins), so a `@card` class can carry fill/stroke/radius/etc. — not only inline `box` attributes. `box` is always styled.
        let pattrs = self.paint_attrs(el);
        let (param, rect_style, opacity_call) = self.rect_style_pieces(&pattrs);

        let has_dynamic = el.children.iter().any(|n| {
            matches!(
                n,
                ViewNode::IfBlock(_) | ViewNode::ForBlock(_) | ViewNode::LetStmt { .. }
            )
        });

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
        let _ = writeln!(
            code,
            "{inner_pad}StyledContainer::new(ctx, {layout_style}, move |{param}| {rect_style}, {children})?{opacity_call}"
        );

        let _ = write!(code, "{pad}}};");
        ChildEmit::Simple { name: var, code }
    }

    /// Builds a `Paint::Gradient(...)` expression for a `box` element, using the closure parameter `r` (the rendered `Bounds`) for absolute gradient points.
    ///
    /// `gradient:horizontal/vertical/diagonal/radial` with `from:` / `to:` (required), optional `mid:` / `mid_pos:`.
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

    fn emit_scroll(&mut self, el: &Element) -> ChildEmit {
        let var = self.next_variable_name(&el.tag);
        let pad = self.indent_str();
        let style = self.make_layout_style(&el.tag, &el.classes, &el.attributes);

        self.indent += 1;
        let inner_pad = self.indent_str();
        // LayoutScrollArea wraps a single content item; if multiple children exist, wrap them in a column first.
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
                        let _ = writeln!(code, "{inner_pad}__children.push(box_item({name}));");
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
        let var = self.next_variable_name(&el.tag);
        let pad = self.indent_str();
        let style = self.make_layout_style(&el.tag, &el.classes, &el.attributes);
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
            let params = el.canvas_parameters.as_deref().unwrap_or("");
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

    /// Emits a `RenderNode` expression for an element that is a direct child of a `canvas` element. The result is an expression string, not a statement.
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
    /// Attrs: `x`, `y`, `w`, `h` (numbers or `full`), `fill`, `stroke`, `stroke_w`, `radius`, `shadow_x`, `shadow_y`, `shadow_blur`, `shadow_color`, `gradient` (linear/radial), `from`, `to`, `mid`, `mid_pos`, `x1`, `y1`, `x2`, `y2` (linear points), `cx`, `cy`, `r` (radial).
    fn emit_canvas_rect(&self, el: &Element) -> String {
        let x = self.canvas_dim("x", &el.attributes);
        let y = self.canvas_dim("y", &el.attributes);
        let w = self.canvas_dim("w", &el.attributes);
        let h = self.canvas_dim("h", &el.attributes);

        let radius = el
            .attributes
            .iter()
            .find(|a| a.key == "radius")
            .and_then(|a| a.value.parse::<f32>().ok())
            .map(|r| format!("BorderRadius::all({})", format_f32(r)))
            .unwrap_or_else(|| "BorderRadius::zero()".to_string());

        let shadow = self.canvas_shadow(&el.attributes);
        let stroke = el
            .attributes
            .iter()
            .find(|a| a.key == "stroke")
            .map(|a| self.color_expr(&a.value));
        let stroke_w = el
            .attributes
            .iter()
            .find(|a| a.key == "stroke_w")
            .and_then(|a| a.value.parse::<f32>().ok())
            .unwrap_or(1.0);
        let gradient = self.canvas_gradient_paint(&el.attributes);
        let solid_fill = el
            .attributes
            .iter()
            .find(|a| a.key == "fill")
            .map(|a| self.color_expr(&a.value));

        let rect_style = build_rect_style(gradient, solid_fill, stroke, stroke_w, shadow, &radius);

        format!(
            "RenderNode::rect(Rect {{ x: {x}, y: {y}, width: {w}, height: {h} }}, {rect_style})"
        )
    }

    /// Builds a `Paint::Gradient(...)` expression when `gradient:linear` or `gradient:radial` is present. Color stops: `from:` / `to:` (required), optional `mid:` with `mid_pos:` (default 0.5).
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
            .find(|a| a.key == "mid_pos")
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

    /// Extracts `shadow-*` attrs and produces a `Some(Shadow::new(...))` expression, or `None` when no shadow attrs are present.
    fn canvas_shadow(&self, attrs: &[Attr]) -> Option<String> {
        if !attrs.iter().any(|a| a.key.starts_with("shadow")) {
            return None;
        }
        let sx = attrs
            .iter()
            .find(|a| a.key == "shadow_x")
            .and_then(|a| a.value.parse::<f32>().ok())
            .unwrap_or(0.0);
        let sy = attrs
            .iter()
            .find(|a| a.key == "shadow_y")
            .and_then(|a| a.value.parse::<f32>().ok())
            .unwrap_or(4.0);
        let blur = attrs
            .iter()
            .find(|a| a.key == "shadow_blur")
            .and_then(|a| a.value.parse::<f32>().ok())
            .unwrap_or(8.0);
        let color = attrs
            .iter()
            .find(|a| a.key == "shadow_color")
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
            el.attributes
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
            .attributes
            .iter()
            .find(|a| a.key == "color")
            .map(|a| self.color_expr(&a.value))
            .unwrap_or_else(|| "Color::BLACK".to_string());
        let width = el
            .attributes
            .iter()
            .find(|a| a.key == "width" || a.key == "stroke_w")
            .and_then(|a| a.value.parse::<f32>().ok())
            .map(format_f32)
            .unwrap_or_else(|| "1.0".to_string());
        format!(
            "Line::new(move || Point::new({x1}, {y1}), move || Point::new({x2}, {y2}), move || Stroke::new({color}, {width})).view()"
        )
    }

    /// Generates a `RenderNode::layer(opacity, blur, [...])` expression.
    ///
    /// Attrs: `opacity` (default 1.0), `blur` (default 0.0). Children are recursively emitted as canvas render-node expressions.
    fn emit_canvas_layer(&self, el: &Element) -> String {
        let opacity = el
            .attributes
            .iter()
            .find(|a| a.key == "opacity")
            .and_then(|a| a.value.parse::<f32>().ok())
            .map(format_f32)
            .unwrap_or_else(|| "1.0".to_string());
        let blur = el
            .attributes
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

    /// Generates a `RenderNode::text(...)` expression for a `text` element inside a canvas. Uses absolute coordinates unlike layout-mode `text`.
    fn emit_canvas_text(&self, el: &Element) -> String {
        let content = el.content.as_deref().unwrap_or("");
        let x = self.canvas_dim("x", &el.attributes);
        let y = self.canvas_dim("y", &el.attributes);
        let w = self.canvas_dim("w", &el.attributes);
        let h = self.canvas_dim("h", &el.attributes);

        let size = el
            .attributes
            .iter()
            .find(|a| a.key == "size")
            .and_then(|a| a.value.parse::<f32>().ok())
            .map(format_f32)
            .unwrap_or_else(|| "14.0".to_string());

        let color = el
            .attributes
            .iter()
            .find(|a| a.key == "color")
            .map(|a| self.color_expr(&a.value))
            .unwrap_or_else(|| "Color::BLACK".to_string());

        format!(
            "RenderNode::text({}, Rect {{ x: {x}, y: {y}, width: {w}, height: {h} }}, TextStyle::new({size}, {color}))",
            rust_str(content)
        )
    }

    /// Resolves a canvas dimension attribute (`x`, `y`, `w`, `h`). `"full"` maps to `__w` (width axis) or `__h` (height axis). Omitted `w`/`h` default to `__w`/`__h`; omitted `x`/`y` default to `0.0`.
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

    /// Emits an unknown tag as a component function call. No-attr tags generate `name(ctx)?`; tags with attrs generate a `NameProps { … }` struct literal. The component's `.rsx` file must declare a matching `pub struct NameProps`.
    fn emit_component_call(&mut self, el: &Element, tag: &str) -> ChildEmit {
        let var = self.next_variable_name("node");
        let pad = self.indent_str();

        if el.attributes.is_empty() && el.children.is_empty() {
            let code = format!("{pad}let {var} = {tag}(ctx)?;");
            return ChildEmit::Simple { name: var, code };
        }

        let props_type = to_pascal_case(tag) + "Props";
        let fields: Vec<String> = el
            .attributes
            .iter()
            .map(|attr| format!("{}: {}", attr.key, self.component_attr_expr(attr)))
            .collect();
        let code = format!(
            "{pad}let {var} = {tag}(ctx, crate::{props_type} {{ {} }})?;",
            fields.join(", ")
        );
        ChildEmit::Simple { name: var, code }
    }

    /// Converts a component attribute to a Rust expression. Quoted attrs (`label:"text"`) become string literals; numbers become `f32` literals; hex/named colors resolve via `color_expr`; everything else is forwarded verbatim.
    ///
    /// Simple lowercase identifiers (e.g. `fill:primary`) are routed through `color_expr` so they follow the same [style]-vs-theme precedence as built-in elements. PascalCase or complex expressions are passed through verbatim.
    fn component_attr_expr(&self, attr: &Attr) -> String {
        if attr.is_quoted {
            return rust_str(&attr.value);
        }
        let v = attr.value.trim();
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

    fn emit_widget_ref(&mut self, el: &Element) -> ChildEmit {
        let var = el.content.as_deref().unwrap_or("").trim().to_string();
        ChildEmit::Simple {
            name: var,
            code: String::new(),
        }
    }

    /// The effective paint attributes for an element: its inline attrs followed by the paint props of its first class. Inline wins because the paint helpers take the first `.find()` match.
    fn paint_attrs(&self, el: &Element) -> Vec<Attr> {
        let mut attrs = el.attributes.clone();
        if let Some(name) = el.classes.first()
            && let Some(class) = self.classes.iter().find(|c| &c.name == name)
        {
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
        attrs
    }

    /// Builds the `(closure-param, RectStyle expr, .with_opacity(..) suffix)` for a styled container from paint attributes. The param is `r` only when a gradient needs the rendered bounds.
    fn rect_style_pieces(&self, pattrs: &[Attr]) -> (&'static str, String, String) {
        let shadow = self.canvas_shadow(pattrs);
        let gradient = self.box_gradient_paint(pattrs);
        let solid_fill = pattrs
            .iter()
            .find(|a| a.key == "fill")
            .map(|a| self.color_expr(&a.value));
        let stroke = pattrs
            .iter()
            .find(|a| a.key == "stroke")
            .map(|a| self.color_expr(&a.value));
        let stroke_w = pattrs
            .iter()
            .find(|a| a.key == "stroke_w")
            .and_then(|a| a.value.parse::<f32>().ok())
            .unwrap_or(1.0);
        let radius = pattrs
            .iter()
            .find(|a| a.key == "radius")
            .and_then(|a| a.value.parse::<f32>().ok())
            .map(|r| format!("BorderRadius::all({})", format_f32(r)))
            .unwrap_or_else(|| "BorderRadius::zero()".to_string());
        let opacity = pattrs
            .iter()
            .find(|a| a.key == "opacity")
            .and_then(|a| a.value.parse::<f32>().ok());
        let param = if gradient.is_some() { "r" } else { "_" };
        let rect_style = build_rect_style(gradient, solid_fill, stroke, stroke_w, shadow, &radius);
        let opacity_call = opacity
            .map(|o| format!(".with_opacity({})", format_f32(o)))
            .unwrap_or_default();
        (param, rect_style, opacity_call)
    }

    /// Builds the `LayoutStyle` expression for a container: base style from the tag (or a class function), then inline attribute modifiers chained on.
    fn make_layout_style(&self, tag: &str, classes: &[String], attrs: &[Attr]) -> String {
        let mut expr = if let Some(first) = classes.first() {
            // The first class provides the base style; further classes cannot currently compose, so only the first is applied.
            let mut base = format!("{}()", style_function_name(first));
            // Apply the tag's flex direction only when no class declares one, so a styled `row .card` still lays out horizontally.
            if !self.class_has_direction(first) {
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

    fn emit_if(&mut self, block: &IfBlock) -> ChildEmit {
        let pad = self.indent_str();
        let mut code = String::new();
        // The condition is already trimmed by the parser and emitted verbatim, so its span maps directly.
        let cond = block.condition.trim();
        let marker = expr_marker(block.condition_start, cond.len());
        let _ = writeln!(code, "{pad}if {marker}{cond} {{");
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
        let pad = self.indent_str();
        let mut code = String::new();
        let _ = writeln!(
            code,
            "{pad}for {} in {} {{",
            block.pattern.trim(),
            block.iterable.trim()
        );
        self.indent += 1;
        // Loop variables are often borrowed (`items.iter()`), but widget closures require `'static` captures; bind owned copies so they can be moved in.
        let body_pad = self.indent_str();
        let idents = pattern_idents(&block.pattern);
        for ident in &idents {
            let _ = writeln!(code, "{body_pad}let {ident} = {ident}.to_owned();");
        }
        let added = idents.len();
        self.loop_variables.extend(idents);
        self.emit_branch_into_children(&block.body, &mut code);
        self.loop_variables
            .truncate(self.loop_variables.len() - added);
        self.indent -= 1;
        let _ = write!(code, "{pad}}}");
        ChildEmit::Dynamic { code }
    }

    /// Emits a control-flow branch's nodes, pushing every produced widget into the surrounding `__children` vector.
    fn emit_branch_into_children(&mut self, nodes: &[ViewNode], code: &mut String) {
        let pad = self.indent_str();
        for node in nodes {
            match self.emit_node(node) {
                ChildEmit::Simple { name, code: c } => {
                    let _ = writeln!(code, "{c}");
                    let _ = writeln!(code, "{pad}__children.push(box_item({name}));");
                }
                ChildEmit::Dynamic { code: c } => {
                    let _ = writeln!(code, "{c}");
                }
            }
        }
    }

    /// Builds the `content_fn` closure for a text node, handling `{...}` interpolation. `content_start` is the source byte offset of `content`, used to tag each interpolated expression with its source span.
    pub fn interpolate_content(&self, content: &str, content_start: usize) -> String {
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
                Segment::Expr { text, byte_offset } => {
                    fmt.push_str("{}");
                    args.push(self.render_interp_expr(text, content_start + byte_offset));
                }
            }
        }

        let args_joined = args.join(", ");
        format!("move || format!({}, {args_joined})", rust_str(&fmt))
    }

    /// Renders an interpolation expression: a `$ident` reactive read becomes `ident.get()`; a `$`-free expression is emitted verbatim (a plain value). `raw_start` is the source byte offset of the raw (untrimmed) expression text; an [`expr_marker`] is emitted right before a verbatim (`$`-free) expression so the analyzer can complete inside it.
    fn render_interp_expr(&self, expr: &str, raw_start: usize) -> String {
        let trimmed = expr.trim();
        if trimmed.is_empty() {
            return format!("{{ {expr} }}");
        }
        // A `$ident` is a reactive read (`ident.get()`). Substitution rewrites the text, so a `$` expression gets no verbatim span; a `$`-free expression is copied byte-for-byte (a plain, non-reactive value) and keeps its source span for LSP mapping.
        if trimmed.contains('$') {
            return format!("{{ {} }}", substitute_reads(trimmed));
        }
        let lead = expr.len() - expr.trim_start().len();
        let marker = expr_marker(raw_start + lead, trimmed.len());
        if is_ident(trimmed) {
            format!("{marker}{trimmed}")
        } else {
            format!("{{ {marker}{trimmed} }}")
        }
    }

    /// Resolves a color reference: an inline hex value, a CSS keyword, a `Color::*` literal, a `[style]`-declared local constant, or a theme field.
    ///
    /// Lookup order:
    /// 1. Inline hex / `Color::*` / CSS keyword → static expression.
    /// 2. `theme_type` set → `use_theme::<T>().field` (reactive) for every named color, including `[style]`-declared ones, so runtime theme switching takes effect; use inline hex for a true non-theme one-off.
    /// 3. No `theme_type` → file-local `COLOR_*` constant (declared in `[style]`, or rustc catches the missing symbol if undeclared).
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
        if let Some(theme) = &self.theme_type {
            return format!("use_theme::<{theme}>().{}", to_snake_case(v));
        }
        constant_name("COLOR_", v)
    }

    /// Whether codegen resolves any color through `use_theme`, requiring the import.
    pub fn uses_theme(&self) -> bool {
        self.theme_type.is_some()
    }
}

enum Segment {
    Literal(String),
    /// An interpolated `{expr}`: the raw inner text plus the byte offset (within `content`) where it begins, used to map the verbatim expression back to the `.rsx` source.
    Expr {
        text: String,
        byte_offset: usize,
    },
}

/// Splits a string into literal and `{expr}` segments. Escaped braces `{{`/`}}` are treated as literal single braces.
fn parse_interpolation(content: &str) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut literal = String::new();
    let mut chars = content.char_indices().peekable();

    while let Some((idx, c)) = chars.next() {
        match c {
            '{' if chars.peek().map(|&(_, c)| c) == Some('{') => {
                chars.next();
                literal.push('{');
            }
            '}' if chars.peek().map(|&(_, c)| c) == Some('}') => {
                chars.next();
                literal.push('}');
            }
            '{' => {
                if !literal.is_empty() {
                    segments.push(Segment::Literal(std::mem::take(&mut literal)));
                }
                let mut expr = String::new();
                // The expression text begins one byte past this `{`.
                let byte_offset = idx + c.len_utf8();
                for (_, ec) in chars.by_ref() {
                    if ec == '}' {
                        break;
                    }
                    expr.push(ec);
                }
                segments.push(Segment::Expr {
                    text: expr,
                    byte_offset,
                });
            }
            _ => literal.push(c),
        }
    }
    if !literal.is_empty() {
        segments.push(Segment::Literal(literal));
    }
    segments
}

/// Extracts the binding identifiers from a `for` pattern, ignoring tuple punctuation and the `_` wildcard. `(i, item)` -> `["i", "item"]`.
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

/// Renders a Rust string literal, escaping quotes and backslashes.
fn rust_str(s: &str) -> String {
    let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

/// An [`expr_marker`] for a verbatim closure attribute value (one beginning with `|`), or an empty string otherwise. The value is emitted byte-for-byte after `move `, so the span maps directly.
fn closure_marker(attr: Option<&Attr>) -> String {
    let Some(attr) = attr else {
        return String::new();
    };
    let trimmed = attr.value.trim_start();
    if !trimmed.starts_with('|') {
        return String::new();
    }
    let lead = attr.value.len() - trimmed.len();
    expr_marker(attr.value_start + lead, attr.value.trim().len())
}

/// The parser strips `on_press:` leaving `|| expr` or `|ev| expr`. Ensure the value is a closure; wrap bare expressions in a zero-arg closure.
fn normalize_closure(value: &str) -> String {
    let v = value.trim();
    if v.starts_with('|') {
        v.to_string()
    } else {
        format!("|| {{ {v} }}")
    }
}

/// Replaces every `$ident` in `s` with `ident.get()` — a reactive read, for `[view]` interpolation where a signal reference is a value read.
fn substitute_reads(s: &str) -> String {
    substitute_dollar(s, true)
}

/// Replaces every `$ident` in `s` with the bare `ident` (the signal handle), for closure bodies where `$count.update(…)` means the handle and `$` only marks it for cloning.
fn substitute_handles(s: &str) -> String {
    substitute_dollar(s, false)
}

/// Rewrites each `$ident` to `ident` (plus `.get()` when `read`). Only an ASCII `$` followed by an identifier start counts as a marker; everything else is copied through unchanged.
fn substitute_dollar(s: &str, read: bool) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$'
            && bytes
                .get(i + 1)
                .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_')
        {
            let start = i + 1;
            let mut j = start;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            out.push_str(&s[start..j]);
            if read {
                out.push_str(".get()");
            }
            i = j;
        } else {
            let ch = s[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

/// Collects the identifier of every `$ident` signal reference in `s`, used to clone signals captured by a closure.
fn signal_idents(s: &str) -> Vec<String> {
    let bytes = s.as_bytes();
    let mut idents = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$'
            && bytes
                .get(i + 1)
                .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_')
        {
            let start = i + 1;
            let mut j = start;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            idents.push(s[start..j].to_string());
            i = j;
        } else {
            i += 1;
        }
    }
    idents
}

/// Assembles a `&[(pos, color)]` gradient stops expression from the resolved `from`, `to`, and optional `mid`/`mid_pos` values.
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

/// Keys that contribute to a container's paint (`RectStyle`) rather than its layout. Used to pick which class props to merge into an element's paint attributes.
fn is_paint_key(key: &str) -> bool {
    matches!(
        key,
        "fill"
            | "stroke"
            | "stroke_w"
            | "radius"
            | "opacity"
            | "gradient"
            | "from"
            | "to"
            | "mid"
            | "mid_pos"
            | "gr"
    ) || key.starts_with("shadow")
}

/// Whether any paint attribute is present, so a plain `col`/`row` must upgrade to a `StyledContainer`.
fn has_paint(pattrs: &[Attr]) -> bool {
    pattrs.iter().any(|a| {
        matches!(
            a.key.as_str(),
            "fill" | "stroke" | "radius" | "opacity" | "gradient"
        ) || a.key.starts_with("shadow")
    })
}

/// Builds a `RectStyle { … }` or shorthand expression from the resolved fill, stroke, shadow, and radius values. Mirrors the branching logic shared by `emit_box` and `emit_canvas_rect`.
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
            Some(f) => format!("RectStyle::default().with_fill({f}).with_radius({radius})"),
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

    fn make_gen<'a>() -> ViewGen<'a> {
        ViewGen::with_theme(&[], &[], None)
    }

    #[test]
    fn literal_content() {
        let g = make_gen();
        assert_eq!(
            g.interpolate_content("hello", 0),
            "|| \"hello\".to_string()"
        );
    }

    #[test]
    fn signal_interpolation() {
        let g = make_gen();
        // `$count` is a reactive read; the rewritten expression carries no verbatim span.
        assert_eq!(
            g.interpolate_content("Count: {$count}", 0),
            "move || format!(\"Count: {}\", { count.get() })"
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

    #[test]
    fn class_paint_promotes_container_and_is_consumed() {
        let src = "[style]\n@card\n    fill: #ffffff\n    radius: 12\n    padding: 8\n[view]\ncol @card\n    text \"hi\"\n";
        let out = crate::transpile_source_with_theme(src, "demo", None).unwrap();
        let code = &out.rust_code;
        // A `col` carrying paint from its class becomes a StyledContainer, not a plain Container.
        assert!(
            code.contains("StyledContainer::new(ctx, style_card()"),
            "painted col should be a StyledContainer:\n{code}"
        );
        // The class's fill reaches the RectStyle.
        assert!(
            code.contains("with_fill"),
            "class fill should reach the RectStyle:\n{code}"
        );
    }
}
