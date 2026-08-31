//! Editable-text `input` emitter: binds `value:$signal` to an `Input` widget with a text style.

use telar_parser::{Element, Value};

use crate::style::{PropCall, layout_prop_call};

use super::signals::{normalize_closure, rust_str, substitute_handles, wrap_signal_clones};
use super::{ChildEmit, ViewGen};

impl ViewGen<'_> {
    pub(super) fn emit_input(&mut self, el: &Element) -> ChildEmit {
        let var = self.next_variable_name("input");
        let pad = self.indent_str();

        // `value:$signal` binds the field to an `RwSignal<String>`; pass a clone so the binding stays usable
        // elsewhere. A non-`$` value is forwarded verbatim (e.g. an already-owned signal expression).
        let value_expr = match el.attributes.iter().find(|a| a.key == "value") {
            Some(a) => match a.value.text().trim().strip_prefix('$') {
                Some(id) => format!("{id}.clone()"),
                None => a.value.text().trim().to_string(),
            },
            None => "Default::default()".to_string(),
        };

        // Styled by what the tree above it declared, amended by whatever it says for itself — the same rule a
        // `text` takes, and for the same reason: a field that ignored the face around it was written in a
        // different hand from the labels beside it, which is why one standing in for a tab had to stay Rust.
        let mut hoists = Vec::new();
        let transitions = std::collections::HashMap::new();
        let modifiers = self.inheritable_modifiers(&el.attributes, &transitions, &mut hoists);
        let style = wrap_signal_clones(
            &[super::text::raw_color_value(&el.attributes)],
            format!("move |__inherited: TextStyle| __inherited{modifiers}"),
        );
        // The size a leaf falls back to for its own height, which is the one thing inheritance cannot answer
        // before the layout runs.
        let size = el
            .attributes
            .iter()
            .find(|a| a.key == "font_size")
            .map(|a| crate::style::number_or(a.value.text(), "14.0"))
            .unwrap_or_else(|| "14.0".to_string());

        // Remaining attrs are layout (width/height/…); `value`/`size`/`color`/`on_submit` are consumed above.
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

        // Optional `on_submit` closure (Enter), boxed like any handler with its `$signal`s cloned in.
        let on_submit = el
            .attributes
            .iter()
            .find(|a| a.key == "on_submit")
            .map(|a| {
                let closure = substitute_handles(&normalize_closure(a.value.text()));
                wrap_signal_clones(&[a.value.text()], format!("move {closure}"))
            });

        // The hint shown while the field is empty: a quoted literal, a `t"…"` translation, or any expression
        // yielding a `String`. The widget has taken one since it existed; without a spelling for it, every form
        // field in a real application had to stay hand-written Rust just to say what it is for.
        let placeholder = el
            .attributes
            .iter()
            .find(|a| a.key == "placeholder")
            .map(|a| match &a.value {
                Value::Quoted(text) => format!("{}.to_string()", rust_str(text)),
                value => value.text().trim().to_string(),
            });

        // The other half of `on_submit`: Escape is a key a focused field eats, so an application watching from
        // outside cannot tell «they gave up» from «they clicked away» — opposite answers wherever losing focus
        // commits.
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
        // A bare `secret` is the whole of what a password field needs to say; the bullet is not the caller's
        // choice to make. Without a spelling for it, every login form in a `.rsx` application had to drop to
        // hand-written Rust or render the password in clear text.
        if el.attributes.iter().any(|a| a.key == "secret") {
            tail.push_str(".secret()");
        }
        // A field somebody has to click before they can type into it is a field that has not opened — which is
        // what a surface that exists *because* it wants a keystroke is asking for.
        if el.attributes.iter().any(|a| a.key == "autofocus") {
            tail.push_str(".autofocus()");
        }
        // `focus_id:$sig` — who holds the keyboard, published by the field itself, because the answer only
        // exists once the widget does: `[logic]` runs before the view is built and has nothing to ask yet.
        // Withdrawn when the field goes, by the field: an id that outlived the widget it named is an answer
        // about something that is not there any more, and whoever is watching would read it as «still typing».
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
