//! `label:`, `lang:` and `a11y:` on any built-in tag: what the box is called, what language it is in, and whether a reader skips it.

use telar_parser::{Attr, Element, Value};

use super::signals::{rust_str, substitute_reads};
use super::{ChildEmit, ViewGen, expr_marker};

impl ViewGen<'_> {
    /// Chains the annotations an element carries onto the widget it built, through `telar::Accessible`, which every widget with a layout node takes. A component tag is left alone: there `label` is one of its props.
    pub(super) fn accessible_tail(&self, el: &Element, emit: ChildEmit) -> ChildEmit {
        if !crate::registry::is_builtin_tag(&el.tag) {
            return emit;
        }
        let find = |key: &str| el.attributes.iter().find(|a| a.key == key);
        let mut calls = String::new();
        if let Some(label) = find("label") {
            calls.push_str(&format!(".a11y_label({})", self.annotation_text(label)));
        }
        if let Some(lang) = find("lang") {
            calls.push_str(&format!(".a11y_lang({})", self.annotation_text(lang)));
        }
        if let Some(method) = find("a11y").and_then(|a| {
            crate::registry::keyword(crate::registry::A11Y_VALUES, a.value.text().trim())
        }) {
            calls.push_str(&format!(".{method}()"));
        }
        if calls.is_empty() {
            return emit;
        }
        match emit {
            ChildEmit::Simple { name, code } => {
                let pad = self.indent_str();
                let code = format!("{code}\n{pad}let {name} = {name}{calls};");
                ChildEmit::Simple { name, code }
            }
            // A control-flow block has no attributes and a fragment no single node, so neither is reachable.
            other => other,
        }
    }

    /// A closure yielding the attribute's text, re-read when what it reads changes, so a name built from `t!(…)` follows the locale.
    fn annotation_text(&self, attr: &Attr) -> String {
        let value = match &attr.value {
            Value::Quoted(text) => return format!("|| {}", rust_str(text)),
            value => value.text().trim(),
        };
        let inner = super::redundant_parens(value).unwrap_or(value);
        let read = if inner.contains('$') {
            substitute_reads(inner)
        } else {
            let lead = attr.value.text().len() - attr.value.text().trim_start().len()
                + usize::from(inner.len() != value.len());
            format!(
                "{}{inner}",
                expr_marker(attr.value_start + lead, inner.len())
            )
        };
        self.clone_captures(
            &[value],
            format!("move || ::std::string::ToString::to_string(&({read}))"),
        )
    }
}

#[cfg(test)]
#[path = "accessible_test.rs"]
mod tests;
