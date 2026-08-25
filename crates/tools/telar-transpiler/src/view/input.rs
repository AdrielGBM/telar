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

        // Text style (size + color), resolved like `text`; the closure self-clones a `$signal` colour.
        let size = el
            .attributes
            .iter()
            .find(|a| a.key == "size")
            .map(|a| self.scope().number_or(a.value.text(), "14.0"))
            .unwrap_or_else(|| "14.0".to_string());
        let color_attr = el.attributes.iter().find(|a| a.key == "color");
        let color = color_attr
            .map(|a| self.color_expr(a.value.text()))
            .unwrap_or_else(|| "Color::BLACK".to_string());
        let color_raw = color_attr.map(|a| a.value.text()).unwrap_or("");
        let style = wrap_signal_clones(
            &[color_raw],
            format!("move || TextStyle::new({size}, {color})"),
        );

        // Remaining attrs are layout (width/height/…); `value`/`size`/`color`/`on_submit` are consumed above.
        let mut extra = String::new();
        for a in &el.attributes {
            if matches!(
                a.key.as_str(),
                "value" | "size" | "color" | "on_submit" | "placeholder" | "secret"
            ) {
                continue;
            }
            if let PropCall::Call(call) = layout_prop_call(&a.key, a.value.text(), self.scope()) {
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
                Value::I18n(key) => self.i18n_lookup(key),
                Value::Quoted(text) => format!("{}.to_string()", rust_str(text)),
                value => value.text().trim().to_string(),
            });

        let mut tail = String::new();
        if let Some(c) = on_submit {
            tail.push_str(&format!(".on_submit({c})"));
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
        let code =
            format!("{pad}let {var} = Input::new({value_expr}, {layout_style}, {style})?{tail};");
        ChildEmit::Simple { name: var, code }
    }
}
