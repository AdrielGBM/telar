//! Editable-text `input` emitter: binds `value:$signal` to an `Input` widget with a text style.

use telar_parser::{Element, Value};

use crate::style::{PropCall, layout_prop_call};

use super::signals::{normalize_closure, rust_str, substitute_handles, wrap_signal_clones};
use super::{ChildEmit, ViewGen};

impl ViewGen<'_> {
    pub(super) fn emit_input(&mut self, el: &Element) -> ChildEmit {
        let var = self.next_variable_name("input");
        let pad = self.indent_str();

        // Pass a clone so the caller's binding stays usable; a non-`$` value is forwarded verbatim.
        let value_expr = match el.attributes.iter().find(|a| a.key == "value") {
            Some(a) => match a.value.text().trim().strip_prefix('$') {
                Some(id) => format!("{id}.clone()"),
                None => a.value.text().trim().to_string(),
            },
            None => "Default::default()".to_string(),
        };

        let mut hoists = Vec::new();
        let transitions = std::collections::HashMap::new();
        let modifiers = self.inheritable_modifiers(&el.attributes, &transitions, &mut hoists);
        let style = wrap_signal_clones(
            &[super::text::raw_color_value(&el.attributes)],
            format!("move |__inherited: TextStyle| __inherited{modifiers}"),
        );
        let size = el
            .attributes
            .iter()
            .find(|a| a.key == "font_size")
            .map(|a| crate::style::number_or(a.value.text(), "14.0"))
            .unwrap_or_else(|| "14.0".to_string());

        // `value`/`size`/`color`/`on_submit` are consumed above; the rest is layout.
        let mut extra = String::new();
        for a in &el.attributes {
            if matches!(
                a.key.as_str(),
                "value"
                    | "font_size"
                    | "font_family"
                    | "color"
                    | "on_submit"
                    | "on_cancel"
                    | "autofocus"
                    | "focus_id"
                    | "placeholder"
                    | "secret"
            ) {
                continue;
            }
            if let PropCall::Call(call) = layout_prop_call(&a.key, a.value.text()) {
                extra.push_str(&call);
            }
        }
        // A leaf has no intrinsic size, so default the height to one line unless the caller pins it.
        let has_height = el.attributes.iter().any(|a| a.key == "height");
        let layout_style = if has_height {
            format!("LayoutStyle::new(){extra}")
        } else {
            format!("LayoutStyle::new(){extra}.height({size} * 1.4)")
        };

        let on_submit = el
            .attributes
            .iter()
            .find(|a| a.key == "on_submit")
            .map(|a| {
                let closure = substitute_handles(&normalize_closure(a.value.text()));
                wrap_signal_clones(&[a.value.text()], format!("move {closure}"))
            });

        let placeholder = el
            .attributes
            .iter()
            .find(|a| a.key == "placeholder")
            .map(|a| match &a.value {
                Value::Quoted(text) => format!("{}.to_string()", rust_str(text)),
                value => value.text().trim().to_string(),
            });

        // Escape is eaten by a focused field, so without this an application cannot tell giving up from clicking away.
        let on_cancel = el
            .attributes
            .iter()
            .find(|a| a.key == "on_cancel")
            .map(|a| {
                let closure = substitute_handles(&normalize_closure(a.value.text()));
                wrap_signal_clones(&[a.value.text()], format!("move {closure}"))
            });

        let mut tail = String::new();
        if let Some(c) = on_submit {
            tail.push_str(&format!(".on_submit({c})"));
        }
        if let Some(c) = on_cancel {
            tail.push_str(&format!(".on_cancel({c})"));
        }
        if let Some(p) = placeholder {
            tail.push_str(&format!(".placeholder({p})"));
        }
        if el.attributes.iter().any(|a| a.key == "secret") {
            tail.push_str(".secret()");
        }
        if el.attributes.iter().any(|a| a.key == "autofocus") {
            tail.push_str(".autofocus()");
        }
        // Published by the field itself, because the answer exists only once the widget does. Withdrawn with the field: an id outliving its widget would read as "still typing".
        let held = el
            .attributes
            .iter()
            .find(|a| a.key == "focus_id")
            .map(|a| a.value.text().trim().trim_start_matches('$').to_string())
            .filter(|target| !target.is_empty());
        let code = match held {
            None => {
                format!(
                    "{pad}let {var} = Input::declaring({value_expr}, {layout_style}, {style})?{tail};"
                )
            }
            Some(target) => format!(
                "{pad}let {var} = {{\n\
                 {pad}    let __field = Input::declaring({value_expr}, {layout_style}, {style})?{tail};\n\
                 {pad}    let {target} = {target}.clone();\n\
                 {pad}    {target}.set(Some(__field.focus_id()));\n\
                 {pad}    on_cleanup(move || {target}.set(None));\n\
                 {pad}    __field\n\
                 {pad}}};"
            ),
        };
        ChildEmit::Simple { name: var, code }
    }
}
