//! Text-family emitters: `text`, `heading`, and `section`, plus their shared child-collection and signal-clone helpers.

use std::collections::HashMap;
use std::fmt::Write;

use rsx_parser::{Attr, Element};

use crate::naming::contains_ident;
use crate::style::{format_f32, layout_prop_call};

use super::signals::{emit_transition_prelude, signal_idents, wrap_signal_clones};
use super::{ChildEmit, ViewGen};

impl ViewGen<'_> {
    pub(super) fn emit_text(&mut self, el: &Element) -> ChildEmit {
        let var = self.next_variable_name(&el.tag);
        let pad = self.indent_str();
        let content = el.content.as_deref().unwrap_or("");
        let content_fn = self.interpolate_content(content, el.content_start);
        let (specs, errors) = self.parse_transitions(el);
        let transitions: HashMap<String, String> = specs.into_iter().collect();
        let mut hoists: Vec<String> = Vec::new();
        let style = self.text_style(&el.attributes, &transitions, &mut hoists);

        let mut extra = String::new();
        for a in &el.attributes {
            if matches!(
                a.key.as_str(),
                "size" | "color" | "weight" | "italic" | "align" | "lines" | "ellipsis" | "height"
            ) {
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
        let inner_pad = format!("{pad}    ");
        let prelude = {
            let mut p = String::new();
            emit_transition_prelude(&mut p, &inner_pad, &errors, &hoists);
            p
        };

        let code = format!(
            "{pad}let {var} = {{\n\
             {clones}\
             {prelude}\
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

    /// Emits the children of a container-like element into `code` and returns the expression to pass as the constructor's children argument. `seed` names are prepended before the emitted children (e.g. a `section`'s heading). When any child is dynamic control flow, this builds a mutable `__children` vec and returns `__children`; otherwise it returns a `children![...]` literal. Used by `emit_container`/`emit_box`; `emit_scroll` differs and is intentionally excluded.
    pub(super) fn emit_children_collection(
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
    pub(super) fn clone_bindings(&self, snippets: &[&str], pad: &str, extra: &str) -> String {
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

    fn text_style(
        &mut self,
        attrs: &[Attr],
        transitions: &HashMap<String, String>,
        hoists: &mut Vec<String>,
    ) -> String {
        let size = attrs
            .iter()
            .find(|a| a.key == "size")
            .and_then(|a| a.value.parse::<f32>().ok())
            .map(format_f32)
            .unwrap_or_else(|| "14.0".to_string());
        let color_attr = attrs.iter().find(|a| a.key == "color");
        let mut color = color_attr
            .map(|a| self.color_expr(&a.value))
            .unwrap_or_else(|| "Color::BLACK".to_string());
        if let Some(curve) = transitions.get("color") {
            color = self.wrap_transition(curve, &color, hoists);
        }

        // Rich-text modifiers: weight (keyword or numeric), italic (flag or bool), align (keyword).
        let mut modifiers = String::new();
        if let Some(w) = attrs
            .iter()
            .find(|a| a.key == "weight")
            .and_then(|a| parse_weight(&a.value))
        {
            modifiers.push_str(&format!(".with_weight({w})"));
        }
        if let Some(a) = attrs.iter().find(|a| a.key == "italic") {
            // A bare `italic` flag (empty value) or `italic:true` turns it on; `italic:false` is the default.
            let v = a.value.trim();
            if v.is_empty() || v == "true" {
                modifiers.push_str(".with_italic(true)");
            }
        }
        if let Some(variant) = attrs
            .iter()
            .find(|a| a.key == "align")
            .and_then(|a| parse_text_align(&a.value))
        {
            modifiers.push_str(&format!(".with_align(TextAlign::{variant})"));
        }
        if let Some(n) = attrs
            .iter()
            .find(|a| a.key == "lines")
            .and_then(|a| a.value.trim().parse::<u16>().ok())
        {
            modifiers.push_str(&format!(".with_max_lines({n})"));
        }
        if let Some(a) = attrs.iter().find(|a| a.key == "ellipsis") {
            let v = a.value.trim();
            if v.is_empty() || v == "true" {
                modifiers.push_str(".with_ellipsis(true)");
            }
        }

        let closure = format!("move || TextStyle::new({size}, {color}){modifiers}");
        // `color_attr`'s raw value (not `color`, already substituted by `color_expr`) is scanned for `$ident` so a signal-backed color clones itself into this closure, leaving the outer binding usable by sibling widgets.
        let raw_color = color_attr.map(|a| a.value.as_str()).unwrap_or("");
        wrap_signal_clones(&[raw_color], closure)
    }
}

/// Maps a `weight:` value — a keyword (`thin`…`black`) or a numeric 100–900 — to the OpenType weight number.
fn parse_weight(value: &str) -> Option<String> {
    let v = value.trim();
    if let Ok(n) = v.parse::<u16>() {
        return Some(n.to_string());
    }
    let n = match v {
        "thin" | "hairline" => 100,
        "extralight" | "extra-light" | "ultralight" => 200,
        "light" => 300,
        "normal" | "regular" => 400,
        "medium" => 500,
        "semibold" | "semi-bold" | "demibold" => 600,
        "bold" => 700,
        "extrabold" | "extra-bold" | "ultrabold" => 800,
        "black" | "heavy" => 900,
        _ => return None,
    };
    Some(n.to_string())
}

/// Maps an `align:` value to a `TextAlign` variant name.
fn parse_text_align(value: &str) -> Option<&'static str> {
    Some(match value.trim() {
        "left" | "start" => "Start",
        "center" | "centre" => "Center",
        "right" | "end" => "End",
        "justify" | "justified" => "Justify",
        _ => return None,
    })
}
