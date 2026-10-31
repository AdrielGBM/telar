//! `label:`, `lang:`, `a11y:`, `anchor:` and `view_progress:` on any built-in tag: what the box is called, what language it is in, whether a reader skips it, the name a link reaches it by, and how far it has scrolled through the view.

use telar_parser::{Attr, Element, Value};

use super::signals::substitute_reads;
use super::{ChildEmit, ViewGen, expr_marker};

impl ViewGen<'_> {
    /// Chains the annotations an element carries onto the widget it built, through `telar::Accessible`, which every widget with a layout node takes. A component call says them about the item it returns, spelled `a11y_label`/`a11y_lang` there because `label` and `lang` are its own props to declare, and may also say what that item is with a `role:` that only describes it.
    pub(super) fn accessible_tail(&mut self, el: &Element, emit: ChildEmit) -> ChildEmit {
        let find = |key: &str| el.attributes.iter().find(|a| a.key == key);
        if !crate::registry::is_builtin_tag(&el.tag) {
            let mut calls = String::new();
            if let Some(label) = find("a11y_label") {
                calls.push_str(&format!(".a11y_label({})", self.annotation_text(label)));
            }
            if let Some(lang) = find("a11y_lang") {
                calls.push_str(&format!(".a11y_lang({})", self.annotation_text(lang)));
            }
            calls.push_str(
                &self.role_annotation(find("role"), crate::registry::COMPONENT_ROLE_VALUES),
            );
            calls.push_str(&self.a11y_keyword_call(find("a11y")));
            calls.push_str(&self.class_authored_text(&el.classes));
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
        if el.tag == "text" {
            calls.push_str(&self.role_annotation(find("role"), crate::registry::TEXT_ROLE_VALUES));
        }
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

    /// The `.authored_text(…)` a component call's classes put on the item it returns: their text properties, applied over what the component says about its own text, where an inline attribute there is a prop. Nothing when they name none.
    fn class_authored_text(&mut self, classes: &[String]) -> String {
        let attrs: Vec<Attr> = self
            .class_attrs(classes)
            .into_iter()
            .filter(|attr| crate::registry::is_inheritable_text_attr(&attr.key))
            .collect();
        let mut hoists = Vec::new();
        let modifiers = self.inheritable_modifiers(
            &attrs,
            &std::collections::HashMap::new(),
            &mut hoists,
            super::text::StyleTarget::Declared,
        );
        if modifiers.is_empty() {
            return String::new();
        }
        let closure = format!("move || Declared::default(){modifiers}");
        format!(
            ".authored_text({})",
            self.owning(&super::text::raw_reactive_values(&attrs), &closure)
        )
    }

    /// A `role:` that only describes what it is written on, by one of `table`'s names or as an expression yielding the `Role`. A value that is neither emits nothing here: the value check has already said why.
    fn role_annotation(
        &self,
        attr: Option<&Attr>,
        table: &'static [(&'static str, &'static str)],
    ) -> String {
        let Some(attr) = attr else {
            return String::new();
        };
        match crate::style::keyword_or_expression(
            "role",
            attr.value.text(),
            table,
            self.theme_access,
        ) {
            Ok(crate::style::Spelled::Keyword(variant)) => {
                format!(".a11y_role(::telar::Role::{variant})")
            }
            Ok(crate::style::Spelled::Expression(role)) => format!(".a11y_role({role})"),
            Err(_) => String::new(),
        }
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
        self.owning(
            &[value],
            &format!("move || ::std::string::ToString::to_string(&({read}))"),
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
                self.owning(&[value], &format!("move || {read}"))
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
