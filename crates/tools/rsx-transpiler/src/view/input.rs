//! Editable-text `input` emitter: binds `value:$signal` to an `Input` widget with a text style.

use rsx_parser::Element;

use crate::style::{format_f32, layout_prop_call};

use super::signals::{normalize_closure, substitute_handles, wrap_signal_clones};
use super::{ChildEmit, ViewGen};

impl ViewGen<'_> {
    pub(super) fn emit_input(&mut self, el: &Element) -> ChildEmit {
        let var = self.next_variable_name("input");
        let pad = self.indent_str();

        // `value:$signal` binds the field to an `RwSignal<String>`; pass a clone so the binding stays usable
        // elsewhere. A non-`$` value is forwarded verbatim (e.g. an already-owned signal expression).
        let value_expr = match el.attributes.iter().find(|a| a.key == "value") {
            Some(a) => match a.value.trim().strip_prefix('$') {
                Some(id) => format!("{id}.clone()"),
                None => a.value.trim().to_string(),
            },
            None => "Default::default()".to_string(),
        };

        // Text style (size + color), resolved like `text`; the closure self-clones a `$signal` colour.
        let size = el
            .attributes
            .iter()
            .find(|a| a.key == "size")
            .and_then(|a| a.value.parse::<f32>().ok())
            .map(format_f32)
            .unwrap_or_else(|| "14.0".to_string());
        let color_attr = el.attributes.iter().find(|a| a.key == "color");
        let color = color_attr
            .map(|a| self.color_expr(&a.value))
            .unwrap_or_else(|| "Color::BLACK".to_string());
        let color_raw = color_attr.map(|a| a.value.as_str()).unwrap_or("");
        let style = wrap_signal_clones(
            &[color_raw],
            format!("move || TextStyle::new({size}, {color})"),
        );

        // Remaining attrs are layout (width/height/…); `value`/`size`/`color`/`on_submit` are consumed above.
        let mut extra = String::new();
        for a in &el.attributes {
            if matches!(a.key.as_str(), "value" | "size" | "color" | "on_submit") {
                continue;
            }
            if let Some(call) = layout_prop_call(&a.key, &a.value) {
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
                let closure = substitute_handles(&normalize_closure(&a.value));
                wrap_signal_clones(&[a.value.as_str()], format!("move {closure}"))
            });

        let tail = match on_submit {
            Some(c) => format!(".on_submit({c})"),
            None => String::new(),
        };
        let code =
            format!("{pad}let {var} = Input::new({value_expr}, {layout_style}, {style})?{tail};");
        ChildEmit::Simple { name: var, code }
    }
}
