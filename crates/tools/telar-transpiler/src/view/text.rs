//! Text-family emitters: `text`, `heading`, and `section`, plus their shared child-collection and signal-clone helpers.

use std::collections::HashMap;
use std::fmt::Write;

use telar_parser::{Attr, Element, Value, ViewNode};

use crate::registry;
use crate::style::{PropCall, layout_prop_call, number_or};

use super::signals::{captured_idents, emit_transition_prelude, rust_str, wrap_signal_clones};
use super::{ChildEmit, ChildMode, ViewGen};

/// The size a `font_size:` value falls back to when it cannot be resolved. A `text` naming no size takes the inherited one instead, so this only ever stands in for a value already reported as an error.
const DEFAULT_SIZE: &str = "14.0";

impl ViewGen<'_> {
    pub(super) fn emit_text(&mut self, el: &Element) -> ChildEmit {
        if !el.children.is_empty() {
            return self.emit_text_runs(el);
        }
        let var = self.next_variable_name(&el.tag);
        let pad = self.indent_str();
        let content = el.content.as_deref().unwrap_or("");
        let content_fn = if el.content_i18n {
            format!("move || {}", self.i18n_lookup(content))
        } else {
            self.interpolate_content(content, el.content_start)
        };
        let (specs, mut errors) = self.parse_transitions(el);
        let transitions: HashMap<String, String> = specs.into_iter().collect();
        let mut hoists: Vec<String> = Vec::new();
        // Regression: the class reached a container and stopped there, so `@heading { font_size: 22 }` compiled and did nothing.
        let attrs = self.effective_attrs(el);
        let style = self.text_style(&attrs, &transitions, &mut hoists);
        let fit = fit_closure(&attrs, &mut errors);
        let (constructor, fit_arg) = fit_constructor("Text::declaring", "Text::fitting", fit, &pad);

        let layout_style = text_layout_style(&attrs);

        // Each `move` closure consumes its captures, so clone into block locals. Scan the raw `content`, still carrying `$`, not the substituted `content_fn`.
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
             {pad}    {constructor}(\n\
             {pad}        {content_fn},\n\
             {pad}        {layout_style},\n\
             {fit_arg}\
             {pad}        {style},\n\
             {pad}    )?\n\
             {pad}}};"
        );
        ChildEmit::Simple { name: var, code }
    }

    /// A `text` whose children are `span`s: one paragraph written as runs, each its own string, restyled by the text properties it names and made a link by `to:`. The text's own quoted content, if any, is the first run.
    fn emit_text_runs(&mut self, el: &Element) -> ChildEmit {
        let var = self.next_variable_name(&el.tag);
        let pad = self.indent_str();
        let (specs, mut errors) = self.parse_transitions(el);
        let transitions: HashMap<String, String> = specs.into_iter().collect();
        let mut hoists: Vec<String> = Vec::new();
        let attrs = self.effective_attrs(el);
        let style = self.text_style(&attrs, &transitions, &mut hoists);
        let layout_style = text_layout_style(&attrs);
        let fit = fit_closure(&attrs, &mut errors);
        let (constructor, fit_arg) = fit_constructor("Text::runs", "Text::runs_fitting", fit, &pad);

        let mut runs: Vec<String> = Vec::new();
        if let Some(content) = el.content.as_deref().filter(|content| !content.is_empty()) {
            runs.push(format!(
                "TextRun::new({})",
                self.run_content(content, el.content_start, el.content_i18n)
            ));
        }
        for child in &el.children {
            let span = match child {
                ViewNode::Element(span) if span.tag == "span" => span,
                ViewNode::Comment(_) => continue,
                _ => {
                    errors.push("a `text` holds `span`s and nothing else".to_string());
                    continue;
                }
            };
            let content = span.content.as_deref().unwrap_or("");
            let mut run = format!(
                "TextRun::new({})",
                self.run_content(content, span.content_start, span.content_i18n)
            );
            let span_attrs = self.effective_attrs(span);
            let modifiers = self.inheritable_modifiers(
                &span_attrs,
                &HashMap::new(),
                &mut hoists,
                StyleTarget::Declared,
            );
            if !modifiers.is_empty() {
                let closure = format!("move || Declared::default(){modifiers}");
                let _ = write!(
                    run,
                    ".declaring({})",
                    wrap_signal_clones(&raw_reactive_values(&span_attrs), closure)
                );
            }
            if let Some(to) = span_attrs.iter().find(|a| a.key == "to") {
                let value = to.value.text().trim();
                if let Some(literal) = super::container::external_literal(value)
                    && semantics_core::Uri::parse(literal).is_none()
                {
                    errors.push(format!(
                        "`to:external(\"{literal}\")` is not an absolute URI: it names no scheme (`https:`, `mailto:`…)"
                    ));
                }
                let read = super::signals::substitute_reads(value);
                let _ = write!(
                    run,
                    ".to({})",
                    wrap_signal_clones(&[value], format!("move || {read}"))
                );
            }
            runs.push(run);
        }

        let clones = self.clone_bindings(&[style.as_str()], &pad, "    ");
        let inner_pad = format!("{pad}    ");
        let mut prelude = String::new();
        emit_transition_prelude(&mut prelude, &inner_pad, &errors, &hoists);
        let runs = runs.join(&format!(",\n{pad}            "));
        let code = format!(
            "{pad}let {var} = {{\n\
             {clones}\
             {prelude}\
             {pad}    {constructor}(\n\
             {pad}        vec![\n\
             {pad}            {runs},\n\
             {pad}        ],\n\
             {pad}        {layout_style},\n\
             {fit_arg}\
             {pad}        {style},\n\
             {pad}    )?\n\
             {pad}}};"
        );
        ChildEmit::Simple { name: var, code }
    }

    /// A run's string as the closure `TextRun::new` takes, with the signals it interpolates cloned in, so runs reading the same signal each own their copy.
    fn run_content(&self, content: &str, start: usize, i18n: bool) -> String {
        let closure = if i18n {
            format!("move || {}", self.i18n_lookup(content))
        } else {
            self.interpolate_content(content, start)
        };
        wrap_signal_clones(&[content], closure)
    }

    /// Emits the children of a container-like element into `code` and returns the expression to pass as the constructor's children argument. `seed` names are prepended (e.g. a `section`'s heading). The `mode` (from [`ViewGen::child_mode`]) picks the shape: [`ChildMode::Slots`] builds a `Vec<ChildSlot>` (`__slots`, for `from_slots`) when a reactive fragment is present, [`ChildMode::Vec`] a `Vec<Box<dyn LayoutItem>>` (`__children`, for `new`) for static control flow, and [`ChildMode::Literal`] a `children![...]`. The caller must have wrapped child emission in the matching [`ViewGen::with_child_sink`] so any `if`/`for` bodies pushed the same shape.
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
            let _ = writeln!(
                out,
                "{pad}{extra}let {}{name} = {name}.clone();",
                super::shadow_marker(&name)
            );
        }
        out
    }

    fn text_style(
        &mut self,
        attrs: &[Attr],
        transitions: &HashMap<String, String>,
        hoists: &mut Vec<String>,
    ) -> String {
        let mut modifiers =
            self.inheritable_modifiers(attrs, transitions, hoists, StyleTarget::Text);
        let asserted = |key: &str| attrs.iter().any(|a| a.key == key && a.value.is_flag());
        // One call, because they are one decision: `ellipsis` without `lines` did nothing, the clamp returning first. Not inheritable — clamping a subtree to two lines means nothing.
        if let Some(lines) = attrs
            .iter()
            .find(|a| a.key == "lines")
            .map(|a| a.value.text().trim().to_string())
            .filter(|value| !value.is_empty())
        {
            modifiers.push_str(&format!(
                ".with_clamp({}, {})",
                crate::style::format_integer(&lines),
                asserted("ellipsis")
            ));
        }

        let closure = format!("move |__inherited: TextStyle| __inherited{modifiers}");
        // The raw value, not the substituted expression, so a signal-backed colour clones itself into this closure.
        wrap_signal_clones(&raw_reactive_values(attrs), closure)
    }

    /// The builder calls for the text properties that flow down a tree, in the spelling both `TextStyle` and `Declared` answer to — which is what lets a `text` and the container above it be written the same way and mean the same thing at different reaches.
    pub(super) fn inheritable_modifiers(
        &mut self,
        attrs: &[Attr],
        transitions: &HashMap<String, String>,
        hoists: &mut Vec<String>,
        target: StyleTarget,
    ) -> String {
        // Amends what the tree declared rather than building a style from nothing; baking a literal here is why a theme could not say "make the body text 11px".
        let mut modifiers = String::new();
        if let Some(size) = attrs.iter().find(|a| a.key == "font_size") {
            match (font_fit(size.value.text()), target) {
                (Some(_), StyleTarget::Text) => {}
                (Some(_), StyleTarget::Declared) => modifiers.push_str(
                    ".with_font_size(::core::compile_error!(\"`font_size:fit(…)` fits one line to a width, so it goes on a `text`, not on what holds or runs inside one\"))",
                ),
                (None, _) => modifiers.push_str(&length_modifier(
                    "with_font_size",
                    size.value.text(),
                    DEFAULT_SIZE,
                    target,
                )),
            }
        }
        let color_attr = attrs.iter().find(|a| a.key == "color");
        if let Some(a) = color_attr {
            let mut color = self.color_expr(a.value.text(), Some(a.value_start));
            if let Some(curve) = transitions.get("color") {
                color = self.wrap_transition(curve, &color, hoists);
            }
            let _ = write!(modifiers, ".with_color({color})");
        }
        if let Some(family) = attrs.iter().find(|a| a.key == "font_family") {
            modifiers.push_str(&format!(".with_font_family({})", font_family_expr(family)));
        }
        if let Some(w) = attrs
            .iter()
            .find(|a| a.key == "font_weight")
            .and_then(|a| parse_weight(a.value.text()))
        {
            modifiers.push_str(&format!(".with_font_weight({w})"));
        }
        if let Some(variant) = attrs
            .iter()
            .find(|a| a.key == "font_style")
            .and_then(|a| registry::keyword(registry::FONT_STYLE_VALUES, a.value.text().trim()))
        {
            modifiers.push_str(&format!(".with_font_style({variant})"));
        }
        if let Some(variant) = attrs
            .iter()
            .find(|a| a.key == "text_align")
            .and_then(|a| registry::keyword(registry::TEXT_ALIGN_VALUES, a.value.text().trim()))
        {
            modifiers.push_str(&format!(".with_text_align({variant})"));
        }
        if let Some(variant) = attrs
            .iter()
            .find(|a| a.key == "text_wrap")
            .and_then(|a| registry::keyword(registry::TEXT_WRAP_VALUES, a.value.text().trim()))
        {
            modifiers.push_str(&format!(".with_text_wrap({variant})"));
        }
        if let Some(lh) = attrs
            .iter()
            .find(|a| a.key == "line_height")
            .map(|a| a.value.text().trim().to_string())
            .filter(|value| !value.is_empty())
        {
            modifiers.push_str(&format!(".with_line_height({})", number_or(&lh, "1.0")));
        }
        if let Some(ls) = attrs
            .iter()
            .find(|a| a.key == "letter_spacing")
            .map(|a| a.value.text().trim().to_string())
            .filter(|value| !value.is_empty())
        {
            modifiers.push_str(&length_modifier("with_letter_spacing", &ls, "0.0", target));
        }
        if let Some(attr) = attrs.iter().find(|a| a.key == "font_variation") {
            let mut value = font_settings_expr(attr, FontSetting::Variations);
            if let Some(curve) = transitions.get("font_variation") {
                value = self.wrap_transition(curve, &value, hoists);
            }
            let _ = write!(modifiers, ".with_font_variations({value})");
        }
        if let Some(attr) = attrs.iter().find(|a| a.key == "font_features") {
            let value = font_settings_expr(attr, FontSetting::Features);
            let _ = write!(modifiers, ".with_font_features({value})");
        }
        if let Some(variant) = attrs
            .iter()
            .find(|a| a.key == "raster")
            .and_then(|a| registry::keyword(registry::RASTER_VALUES, a.value.text().trim()))
        {
            modifiers.push_str(&format!(".with_raster({variant})"));
        }
        if let Some(variant) = attrs
            .iter()
            .find(|a| a.key == "text_case")
            .and_then(|a| registry::keyword(registry::TEXT_CASE_VALUES, a.value.text().trim()))
        {
            modifiers.push_str(&format!(".with_text_case({variant})"));
        }
        if let Some(attr) = attrs.iter().find(|a| a.key == "underline") {
            let value = attr.value.text().trim();
            let drawn = if value.is_empty() {
                "true".to_string()
            } else {
                super::signals::substitute_reads(value)
            };
            let _ = write!(modifiers, ".with_underline({drawn})");
        }
        for (key, method) in [
            ("underline_offset", "with_underline_offset"),
            ("underline_thickness", "with_underline_thickness"),
        ] {
            let Some(value) = attrs
                .iter()
                .find(|a| a.key == key)
                .map(|a| a.value.text().trim().to_string())
                .filter(|value| !value.is_empty())
            else {
                continue;
            };
            if value.contains('$') {
                let read = super::signals::substitute_reads(&value);
                let _ = write!(modifiers, ".{method}(({read}) as f32)");
            } else {
                modifiers.push_str(&length_modifier(method, &value, "0.0", target));
            }
        }
        if let Some(a) = attrs.iter().find(|a| a.key == "underline_color") {
            let color = self.color_expr(a.value.text(), Some(a.value_start));
            let _ = write!(modifiers, ".with_underline_color({color})");
        }
        modifiers
    }

    /// The `.declaring(…)` a container wears, or nothing at all when it names no property that inherits.
    pub(super) fn declaring_call(
        &mut self,
        attrs: &[Attr],
        transitions: &HashMap<String, String>,
        hoists: &mut Vec<String>,
    ) -> String {
        let modifiers =
            self.inheritable_modifiers(attrs, transitions, hoists, StyleTarget::Declared);
        if modifiers.is_empty() {
            return String::new();
        }
        let closure = format!("move || Declared::default(){modifiers}");
        format!(
            ".declaring({})",
            wrap_signal_clones(&raw_reactive_values(attrs), closure)
        )
    }
}

/// The values of the inheritable properties that may read state, as the author wrote them: scanned for `$ident` so each signal they read clones itself into the style closure.
pub(super) fn raw_reactive_values(attrs: &[Attr]) -> Vec<&str> {
    [
        "color",
        "font_variation",
        "font_features",
        "underline",
        "underline_offset",
        "underline_thickness",
        "underline_color",
    ]
    .iter()
    .filter_map(|key| attrs.iter().find(|a| a.key == *key))
    .map(|a| a.value.text())
    .collect()
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum FontSetting {
    Variations,
    Features,
}

/// `font_variation:(wght 650, wdth $w)` or `font_features:(liga 0, tnum)` as the `FontVariations` or `FontFeatures` it builds: each clause a four-letter tag, then its value, which may read state. A feature named alone is turned on. A tag that is not four letters, or an axis with no value, is a build error naming it.
pub(super) fn font_settings_expr(attr: &Attr, kind: FontSetting) -> String {
    let text = match &attr.value {
        Value::Quoted(text) => text.as_str(),
        value => value.text().trim(),
    };
    let inner = text
        .strip_prefix('(')
        .and_then(|inner| inner.strip_suffix(')'))
        .unwrap_or(text);
    let (ty, cast) = match kind {
        FontSetting::Variations => ("FontVariations", "f32"),
        FontSetting::Features => ("FontFeatures", "u32"),
    };
    let mut expr = format!("{ty}::new()");
    for clause in crate::transition::split_top_level(inner, ',')
        .iter()
        .map(|clause| clause.trim())
        .filter(|clause| !clause.is_empty())
    {
        let (tag, value) = clause
            .split_once(char::is_whitespace)
            .unwrap_or((clause, ""));
        let tag = tag.trim_matches(|c| c == '"' || c == '\'');
        let value = value.trim();
        if tag.len() != 4 || !tag.bytes().all(|b| b.is_ascii_graphic()) {
            return format!(
                "::core::compile_error!(\"`{}:` names `{tag}`, which is not a four-letter tag\")",
                attr.key
            );
        }
        let value = match (value.is_empty(), kind) {
            (true, FontSetting::Features) => "1".to_string(),
            (true, FontSetting::Variations) => {
                return format!(
                    "::core::compile_error!(\"`font_variation:` gives the axis `{tag}` no value\")"
                );
            }
            (false, _) => super::signals::substitute_reads(value),
        };
        let _ = write!(expr, ".with(\"{tag}\", ({value}) as {cast})");
    }
    expr
}

/// The family a `font_family:` names, as an expression `TextStyle::with_font_family` accepts.
///
/// A bare or quoted generic (`monospace`, `"monospace"`) names its `FontFamily` variant either way — the DSL's names are Telar's own vocabulary, not CSS text kept apart by quoting. A quoted comma list (`"Inter, sans_serif"`) is an ordered fallback, [`registry::FONT_FAMILY_VALUES`] resolving each name that is one and `FontFamily::from` carrying the rest through as faces. Anything else — an unquoted expression that names no generic — is the author's own Rust: a `theme.font()` read, a `[logic]` binding, carried through because the families an application has are its own vocabulary and not one the DSL can enumerate.
pub(super) fn font_family_expr(attr: &Attr) -> String {
    match &attr.value {
        Value::Quoted(text) => quoted_font_family_expr(text),
        value => {
            let raw = value.text().trim();
            registry::keyword(registry::FONT_FAMILY_VALUES, raw)
                .map(str::to_string)
                .unwrap_or_else(|| raw.to_string())
        }
    }
}

/// A quoted `font_family:` value split on commas, CSS's own separator for a fallback list: one name is that face (or its generic), several become an ordered `FontFamily::stack(…)`.
fn quoted_font_family_expr(text: &str) -> String {
    let names: Vec<&str> = text
        .split(',')
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .collect();
    match names.as_slice() {
        [] => "FontFamily::SansSerif".to_string(),
        [one] => registry::keyword(registry::FONT_FAMILY_VALUES, one)
            .map(str::to_string)
            .unwrap_or_else(|| rust_str(one)),
        many => {
            let pieces: Vec<String> = many
                .iter()
                .map(|name| font_family_stack_piece(name))
                .collect();
            format!("FontFamily::stack([{}])", pieces.join(", "))
        }
    }
}

/// One entry of a fallback list: a generic's constant, already a `FontFamily`, or a face name converted into one.
fn font_family_stack_piece(name: &str) -> String {
    match registry::keyword(registry::FONT_FAMILY_VALUES, name) {
        Some(variant) => variant.to_string(),
        None => format!("{}.into()", rust_str(name)),
    }
}

/// Maps a `font_weight:` value — a keyword (`thin`…`black`) or a numeric 100–900 — to the OpenType weight number.
fn parse_weight(value: &str) -> Option<String> {
    let v = value.trim();
    if let Ok(n) = v.parse::<u16>() {
        return Some(n.to_string());
    }
    registry::keyword(registry::FONT_WEIGHT_VALUES, v).map(str::to_string)
}

/// The layout a `text` asks for: the keys that place it among its siblings, and a `height:` that pins the box over what it measures. Everything that styles the glyphs is the text style's, not the box's.
fn text_layout_style(attrs: &[Attr]) -> String {
    let mut extra = String::new();
    for a in attrs {
        if matches!(
            a.key.as_str(),
            "font_size"
                | "font_family"
                | "color"
                | "font_weight"
                | "font_style"
                | "text_align"
                | "lines"
                | "ellipsis"
                | "text_wrap"
                | "height"
                | "line_height"
                | "letter_spacing"
                | "raster"
                | "font_variation"
                | "font_features"
                | "text_case"
                | "underline"
                | "underline_offset"
                | "underline_thickness"
                | "underline_color"
        ) {
            continue;
        }
        if let PropCall::Call(call) = layout_prop_call(&a.key, a.value.text()) {
            extra.push_str(&call);
        }
    }
    // A leaf measures its own height from the wrapped content; an explicit `height:` pins the box over that.
    let explicit_height = attrs
        .iter()
        .find(|a| a.key == "height")
        .and_then(|a| match layout_prop_call("height", a.value.text()) {
            PropCall::Call(call) => Some(call),
            _ => None,
        })
        .unwrap_or_default();
    format!("LayoutStyle::new(){explicit_height}{extra}")
}

/// Which of the two style types a run of text-property modifiers is chained onto: a `text`'s own `TextStyle`, which resolves a length where it stands, or the `Declared` a container or a span says for what lies beneath it, which the tree resolves.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum StyleTarget {
    Text,
    Declared,
}

/// A `font_size:` or `letter_spacing:` value written with a unit (`22sw`, `1.5em`), as the `TextLength` it names, and whether it is a fraction of the surface.
fn text_length(value: &str) -> Option<(String, bool)> {
    let value = value.trim();
    if let Some((variant, n)) = crate::style::surface_fraction(value) {
        let fraction = crate::style::format_f32(n / 100.0);
        return Some((format!("TextLength::{variant}({fraction})"), true));
    }
    let em = value.strip_suffix("em")?.trim().parse::<f32>().ok()?;
    Some((
        format!("TextLength::Em({})", crate::style::format_f32(em)),
        false,
    ))
}

/// The inside of a `font_size:fit(…)`, or `None` when the size is not a fit.
pub(super) fn font_fit(value: &str) -> Option<&str> {
    value.trim().strip_prefix("fit(")?.strip_suffix(')')
}

/// The `FontFit` a `fit(…)` builds: a width — a percentage of the box holding the text, a text length, or an expression yielding a `FitWidth` — and an optional `max_height:` length.
fn font_fit_expr(inner: &str) -> Result<String, String> {
    let clauses: Vec<String> = crate::transition::split_top_level(inner, ',')
        .into_iter()
        .map(|clause| clause.trim().to_string())
        .filter(|clause| !clause.is_empty())
        .collect();
    let Some((width, rest)) = clauses.split_first() else {
        return Err("`fit()` names no width: write `fit(60%)`, `fit(480)` or `fit(40sw)`".into());
    };
    let mut expr = match width.strip_suffix('%') {
        Some(percent) => match percent.trim().parse::<f32>() {
            Ok(n) => format!(
                "FontFit::containing({})",
                crate::style::format_f32(n / 100.0)
            ),
            Err(_) => return Err(format!("`fit({width})`: `{width}` is not a percentage")),
        },
        None => format!("FontFit::new({})", fit_length(width)?),
    };
    for clause in rest {
        match clause.split_once(':') {
            Some((key, value)) if key.trim() == "max_height" => {
                let _ = write!(expr, ".with_max_height({})", fit_length(value.trim())?);
            }
            _ => {
                return Err(format!(
                    "`fit(…)` takes a width and then `max_height:`, not `{clause}`"
                ));
            }
        }
    }
    Ok(expr)
}

/// A length inside `fit(…)`: pixels, `em` or a fraction of the surface as `font_size:` spells them, or an expression.
fn fit_length(value: &str) -> Result<String, String> {
    if value.is_empty() {
        return Err("`fit(…)` is missing a length".into());
    }
    if let Some((length, _)) = text_length(value) {
        return Ok(length);
    }
    if let Ok(px) = value.parse::<f32>() {
        return Ok(format!("TextLength::Px({})", crate::style::format_f32(px)));
    }
    Ok(super::signals::substitute_reads(value))
}

/// The closure a fitted `text` is built with, or `None` when it names no fit (or a fit that is an error, pushed onto `errors`).
fn fit_closure(attrs: &[Attr], errors: &mut Vec<String>) -> Option<String> {
    let raw = attrs.iter().find(|a| a.key == "font_size")?.value.text();
    match font_fit_expr(font_fit(raw)?) {
        Ok(expr) => Some(wrap_signal_clones(&[raw], format!("move || {expr}"))),
        Err(message) => {
            errors.push(message);
            None
        }
    }
}

/// The constructor a `text` calls and the extra argument line a fit adds to it.
fn fit_constructor(
    plain: &'static str,
    fitting: &'static str,
    fit: Option<String>,
    pad: &str,
) -> (&'static str, String) {
    match fit {
        Some(closure) => (fitting, format!("{pad}        {closure},\n")),
        None => (plain, String::new()),
    }
}

/// `.{method}(…)` for a length: pixels as they always were, a length with a unit resolved where the target resolves it. A `text` reads the surface reactively only for a fraction of it, so a resize re-styles just the text that depends on it.
fn length_modifier(method: &str, value: &str, fallback: &str, target: StyleTarget) -> String {
    match (text_length(value), target) {
        (Some((length, _)), StyleTarget::Declared) => format!(".{method}({length})"),
        (Some((length, surface)), StyleTarget::Text) => {
            let on = if surface {
                "use_surface_size()"
            } else {
                "Size::ZERO"
            };
            format!(".{method}_in({length}, {on})")
        }
        (None, _) => format!(".{method}({})", number_or(value, fallback)),
    }
}
