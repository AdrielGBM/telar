//! Text-family emitters: `text`, `heading`, and `section`, plus their shared child-collection and signal-clone helpers.

use std::collections::HashMap;
use std::fmt::Write;

use telar_parser::{Attr, Element, Value};

use crate::registry;
use crate::style::{PropCall, format_f32, layout_prop_call};

use super::signals::{captured_idents, emit_transition_prelude, rust_str, wrap_signal_clones};
use super::{ChildEmit, ChildMode, ViewGen};

/// The type size a `text` takes when it names none, baked at transpile time. §1.4 of the styles plan is
/// about this number; step 6's inheritance is what replaces it with one the root can set.
const DEFAULT_SIZE: &str = "14.0";

impl ViewGen<'_> {
    pub(super) fn emit_text(&mut self, el: &Element) -> ChildEmit {
        let var = self.next_variable_name(&el.tag);
        let pad = self.indent_str();
        let content = el.content.as_deref().unwrap_or("");
        let content_fn = if el.content_i18n {
            format!("move || {}", self.i18n_lookup(content))
        } else {
            self.interpolate_content(content, el.content_start)
        };
        let (specs, errors) = self.parse_transitions(el);
        let transitions: HashMap<String, String> = specs.into_iter().collect();
        let mut hoists: Vec<String> = Vec::new();
        let style = self.text_style(&el.attributes, &transitions, &mut hoists);

        let mut extra = String::new();
        for a in &el.attributes {
            if matches!(
                a.key.as_str(),
                "size"
                    | "font_family"
                    | "color"
                    | "weight"
                    | "italic"
                    | "align"
                    | "lines"
                    | "ellipsis"
                    | "nowrap"
                    | "height"
                    | "line_height"
                    | "letter_spacing"
                    | "raster"
            ) {
                continue;
            }
            if let PropCall::Call(call) = layout_prop_call(&a.key, a.value.text(), self.scope()) {
                extra.push_str(&call);
            }
        }
        // A leaf measures its own height from the wrapped content, so multi-line text reserves real space; an explicit `height:` pins the box over that, which is all the second constructor ever did.
        let explicit_height = el
            .attributes
            .iter()
            .find(|a| a.key == "height")
            .and_then(
                |a| match layout_prop_call("height", a.value.text(), self.scope()) {
                    PropCall::Call(call) => Some(call),
                    _ => None,
                },
            )
            .unwrap_or_default();
        let layout_style = format!("LayoutStyle::new(){explicit_height}{extra}");

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
             {pad}    Text::new(\n\
             {pad}        {content_fn},\n\
             {pad}        {layout_style},\n\
             {pad}        {style},\n\
             {pad}    )?\n\
             {pad}}};"
        );
        ChildEmit::Simple { name: var, code }
    }

    /// Emits the children of a container-like element into `code` and returns the expression to pass as the
    /// constructor's children argument. `seed` names are prepended (e.g. a `section`'s heading). The `mode`
    /// (from [`ViewGen::child_mode`]) picks the shape: [`ChildMode::Slots`] builds a `Vec<ChildSlot>`
    /// (`__slots`, for `from_slots`) when a reactive fragment is present, [`ChildMode::Vec`] a
    /// `Vec<Box<dyn LayoutItem>>` (`__children`, for `new`) for static control flow, and
    /// [`ChildMode::Literal`] a `children![...]`. The caller must have wrapped child emission in the
    /// matching [`ViewGen::with_child_sink`] so any `if`/`for` bodies pushed the same shape.
    pub(super) fn emit_children_collection(
        &self,
        code: &mut String,
        child_emits: &[ChildEmit],
        inner_pad: &str,
        mode: ChildMode,
        seed: &[String],
    ) -> String {
        match mode {
            ChildMode::Slots => {
                let _ = writeln!(
                    code,
                    "{inner_pad}let mut __slots: Vec<ChildSlot> = Vec::new();"
                );
                for name in seed {
                    let _ = writeln!(
                        code,
                        "{inner_pad}__slots.push(ChildSlot::stat(box_item({name})));"
                    );
                }
                for emit in child_emits {
                    match emit {
                        ChildEmit::Simple { name, code: c } => {
                            let _ = writeln!(code, "{c}");
                            let _ = writeln!(
                                code,
                                "{inner_pad}__slots.push(ChildSlot::stat(box_item({name})));"
                            );
                        }
                        ChildEmit::Fragment { name, code: c } => {
                            let _ = writeln!(code, "{c}");
                            let _ = writeln!(code, "{inner_pad}__slots.push({name});");
                        }
                        ChildEmit::Dynamic { code: c } => {
                            let _ = writeln!(code, "{c}");
                        }
                    }
                }
                "__slots".to_string()
            }
            ChildMode::Vec => {
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
                        // A reactive fragment forces `ChildMode::Slots`, so it never reaches vec mode.
                        ChildEmit::Fragment { code: c, .. } => {
                            let _ = writeln!(code, "{c}");
                        }
                    }
                }
                "__children".to_string()
            }
            ChildMode::Literal => {
                let mut names: Vec<String> = seed.to_vec();
                for emit in child_emits {
                    match emit {
                        ChildEmit::Simple { name, code: c } => {
                            let _ = writeln!(code, "{c}");
                            names.push(name.clone());
                        }
                        ChildEmit::Dynamic { code: c } | ChildEmit::Fragment { code: c, .. } => {
                            let _ = writeln!(code, "{c}");
                        }
                    }
                }
                format!("children![{}]", names.join(", "))
            }
        }
    }

    /// Emits `let name = name.clone();` for every signal (`$name`) referenced in the *raw* `snippets` — still carrying the `$` sigil, so captures are detected before substitution — plus any loop variable in scope they use. Indented under `pad + extra`.
    pub(super) fn clone_bindings(&self, snippets: &[&str], pad: &str, extra: &str) -> String {
        let mut out = String::new();
        for name in captured_idents(snippets, &self.loop_variables) {
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
            .map(|a| self.scope().number_or(a.value.text(), DEFAULT_SIZE))
            .unwrap_or_else(|| DEFAULT_SIZE.to_string());
        let color_attr = attrs.iter().find(|a| a.key == "color");
        let mut color = color_attr
            .map(|a| self.color_expr(a.value.text()))
            .unwrap_or_else(|| "Color::BLACK".to_string());
        if let Some(curve) = transitions.get("color") {
            color = self.wrap_transition(curve, &color, hoists);
        }

        // Rich-text modifiers: weight (keyword or numeric), italic (flag or bool), align (keyword).
        let mut modifiers = String::new();
        // A bare flag (`italic`) is the assertion itself; `italic:true` says the same thing the long way, and anything else — `italic:false` most of all — leaves the default alone.
        let asserted = |key: &str| {
            attrs
                .iter()
                .find(|a| a.key == key)
                .is_some_and(|a| a.value.is_flag() || a.value.text().trim() == "true")
        };
        if let Some(family) = attrs.iter().find(|a| a.key == "font_family") {
            modifiers.push_str(&format!(".with_font_family({})", font_family_expr(family)));
        }
        if let Some(w) = attrs
            .iter()
            .find(|a| a.key == "weight")
            .and_then(|a| parse_weight(a.value.text()))
        {
            modifiers.push_str(&format!(".with_weight({w})"));
        }
        if asserted("italic") {
            modifiers.push_str(".with_italic(true)");
        }
        if let Some(variant) = attrs
            .iter()
            .find(|a| a.key == "align")
            .and_then(|a| registry::keyword(registry::TEXT_ALIGN_VALUES, a.value.text().trim()))
        {
            modifiers.push_str(&format!(".with_align({variant})"));
        }
        if let Some(n) = attrs
            .iter()
            .find(|a| a.key == "lines")
            .and_then(|a| a.value.text().trim().parse::<u16>().ok())
        {
            modifiers.push_str(&format!(".with_max_lines({n})"));
        }
        if asserted("ellipsis") {
            modifiers.push_str(".with_ellipsis(true)");
        }
        // `nowrap`: the label is a token and not prose, so it keeps one line whatever box it is given.
        if asserted("nowrap") {
            modifiers.push_str(".with_no_wrap(true)");
        }
        if let Some(lh) = attrs
            .iter()
            .find(|a| a.key == "line_height")
            .and_then(|a| a.value.text().trim().parse::<f32>().ok())
        {
            modifiers.push_str(&format!(".with_line_height({})", format_f32(lh)));
        }
        if let Some(ls) = attrs
            .iter()
            .find(|a| a.key == "letter_spacing")
            .and_then(|a| a.value.text().trim().parse::<f32>().ok())
        {
            modifiers.push_str(&format!(".with_letter_spacing({})", format_f32(ls)));
        }
        if let Some(variant) = attrs
            .iter()
            .find(|a| a.key == "raster")
            .and_then(|a| registry::keyword(registry::RASTER_VALUES, a.value.text().trim()))
        {
            modifiers.push_str(&format!(".with_raster({variant})"));
        }

        let closure = format!("move || TextStyle::new({size}, {color}){modifiers}");
        // `color_attr`'s raw value (not `color`, already substituted by `color_expr`) is scanned for `$ident` so a signal-backed color clones itself into this closure, leaving the outer binding usable by sibling widgets.
        let raw_color = color_attr.map(|a| a.value.text()).unwrap_or("");
        wrap_signal_clones(&[raw_color], closure)
    }
}

/// The family a `font_family:` names, as an expression `TextStyle::with_font_family` accepts.
///
/// A quoted literal is the common case; anything else is the author's own Rust — a `theme.font()` read, a
/// `[logic]` binding — carried through, because the families an application has are its own vocabulary and
/// not one the DSL can enumerate.
pub(super) fn font_family_expr(attr: &Attr) -> String {
    match &attr.value {
        Value::Quoted(name) => rust_str(name),
        value => value.text().trim().to_string(),
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
