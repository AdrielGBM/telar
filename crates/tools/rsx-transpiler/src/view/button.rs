//! Button emitter and its `ButtonStyle` closure builder.

use std::fmt::Write;

use rsx_parser::{Attr, Element};

use super::signals::{closure_marker, normalize_closure, rust_str, substitute_handles};
use super::{ChildEmit, ViewGen};

impl ViewGen<'_> {
    pub(super) fn emit_button(&mut self, el: &Element) -> ChildEmit {
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
        // `color_expr` lowered any `$color` inside the style closure to `ident.get()`, dropping the `$`
        // that `signal_idents` keys on; scan the raw fill/outline values too so a reused signal colour is
        // still cloned into the closure instead of moved (which would fail to compile).
        for key in ["fill", "outline"] {
            if let Some(a) = el.attributes.iter().find(|a| a.key == key) {
                snippets.push(&a.value);
            }
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
}
