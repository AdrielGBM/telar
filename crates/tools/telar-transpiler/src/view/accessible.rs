//! `label:`, `lang:`, `a11y:`, `anchor:` and `view_progress:` on any built-in tag: what the box is called, what language it is in, whether a reader skips it, the name a link reaches it by, and how far it has scrolled through the view.

use telar_parser::{Attr, Element, Value};

use super::signals::substitute_reads;
use super::{ChildEmit, ViewGen, expr_marker};

impl ViewGen<'_> {
    /// Chains the annotations an element carries onto the widget it built, through `telar::Accessible`, which every widget with a layout node takes. A component call says them about the item it returns, spelled `a11y_label`/`a11y_lang` there because `label` and `lang` are its own props to declare.
    pub(super) fn accessible_tail(&self, el: &Element, emit: ChildEmit) -> ChildEmit {
        let find = |key: &str| el.attributes.iter().find(|a| a.key == key);
        if !crate::registry::is_builtin_tag(&el.tag) {
            let mut calls = String::new();
            if let Some(label) = find("a11y_label") {
                calls.push_str(&format!(".a11y_label({})", self.annotation_text(label)));
            }
            if let Some(lang) = find("a11y_lang") {
                calls.push_str(&format!(".a11y_lang({})", self.annotation_text(lang)));
            }
            calls.push_str(&self.a11y_keyword_call(find("a11y")));
            return self.chain_calls(emit, &calls);
        }
        let mut calls = String::new();
        if let Some(label) = find("label") {
            // A `scroll` is never a control in markup, so its name is the one a reader hears while it is a Tab stop of its own; `a11y_label` there would name a region that fits as well, said again on the way into it.
            let method = match el.tag.as_str() {
                "scroll" => "stop_label",
                _ => "a11y_label",
            };
            calls.push_str(&format!(".{method}({})", self.annotation_text(label)));
        }
        if let Some(lang) = find("lang") {
            calls.push_str(&format!(".a11y_lang({})", self.annotation_text(lang)));
        }
        calls.push_str(&self.a11y_keyword_call(find("a11y")));
        if let Some(anchor) = find("anchor") {
            calls.push_str(&format!(".page_anchor({})", self.annotation_text(anchor)));
        }
        if let Some(progress) = find("view_progress") {
            let target = progress.value.text().trim().trim_start_matches('$');
            let range = find("view_range")
                .and_then(|a| {
                    crate::registry::keyword(
                        crate::registry::VIEW_RANGE_VALUES,
                        a.value.text().trim(),
                    )
                })
                .unwrap_or("Cover");
            calls.push_str(&format!(
                ".view_progress(ViewRange::{range}, {target}.clone())"
            ));
        }
        self.chain_calls(emit, &calls)
    }

    fn a11y_keyword_call(&self, attr: Option<&Attr>) -> String {
        attr.and_then(|a| {
            crate::registry::keyword(crate::registry::A11Y_VALUES, a.value.text().trim())
        })
        .map(|method| format!(".{method}()"))
        .unwrap_or_default()
    }

    fn chain_calls(&self, emit: ChildEmit, calls: &str) -> ChildEmit {
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

    /// A closure yielding the attribute's text, re-read when what it reads changes, so a name built from `t!(…)` follows the locale. A quoted one interpolates `{…}` as a `text` does.
    fn annotation_text(&self, attr: &Attr) -> String {
        let value = match &attr.value {
            Value::Quoted(text) => return self.text_closure(text, attr.value_start),
            value => value.text().trim(),
        };
        let inner = super::redundant_parens(value).unwrap_or(value);
        let read = if inner.contains('$') {
            substitute_reads(inner, self.theme_access)
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

impl ViewGen<'_> {
    /// `theme:` builds the element inside a scope that provides the theme it names, so the element's own `$theme` reads and everything under it resolve that theme. A value reading `$state` is followed with `follow_theme`.
    pub(super) fn theme_scope(&self, el: &Element, emit: ChildEmit) -> ChildEmit {
        if !crate::registry::is_builtin_tag(&el.tag) {
            return emit;
        }
        let Some(theme) = el.attributes.iter().find(|a| a.key == "theme") else {
            return emit;
        };
        let ChildEmit::Simple { name, code } = emit else {
            return emit;
        };
        let raw = theme.value.text().trim();
        let value = super::redundant_parens(raw).unwrap_or(raw);
        let provided = if value.contains('$') {
            let read = substitute_reads(value, self.theme_access);
            format!(
                "follow_theme({})",
                self.clone_captures(&[value], format!("move || {read}"))
            )
        } else {
            value.to_string()
        };
        let pad = self.indent_str();
        let code = format!(
            "{pad}let {name} = provide_theme({provided}, || {{\n\
             {code}\n\
             {pad}    Ok(Box::new({name}) as Box<dyn LayoutItem>)\n\
             {pad}}})?;"
        );
        ChildEmit::Simple { name, code }
    }
}
