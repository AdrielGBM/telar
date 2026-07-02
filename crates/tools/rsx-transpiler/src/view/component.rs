//! Component-call and `widget` reference emitters.

use rsx_parser::{Attr, Element};

use crate::naming::{is_ident, to_pascal_case, to_snake_case};
use crate::style::{format_f32, hex_to_color_expr};

use super::signals::rust_str;
use super::{ChildEmit, ViewGen, expr_marker};

impl ViewGen<'_> {
    /// Emits an unknown tag as a component function call. No-attr tags generate `name(ctx)?`; tags with attrs generate a `NameProps { … }` struct literal. The component's `.rsx` file must declare a matching `pub struct NameProps`.
    pub(super) fn emit_component_call(&mut self, el: &Element, tag: &str) -> ChildEmit {
        let var = self.next_variable_name("node");
        let pad = self.indent_str();

        if el.attributes.is_empty() && el.children.is_empty() {
            let code = format!("{pad}let {var} = {tag}(ctx)?;");
            return ChildEmit::Simple { name: var, code };
        }

        let props_type = to_pascal_case(tag) + "Props";
        let fields: Vec<String> = el
            .attributes
            .iter()
            .map(|attr| format!("{}: {}", attr.key, self.component_attr_expr(attr)))
            .collect();
        let code = format!(
            "{pad}let {var} = {tag}(ctx, crate::{props_type} {{ {} }})?;",
            fields.join(", ")
        );
        ChildEmit::Simple { name: var, code }
    }

    /// Converts a component attribute to a Rust expression. Quoted attrs (`label:"text"`) become string literals; numbers become `f32` literals; hex/named colors resolve via `color_expr`; everything else is forwarded verbatim.
    ///
    /// Simple lowercase identifiers (e.g. `fill:primary`) are routed through `color_expr` so they follow the same [style]-vs-theme precedence as built-in elements. PascalCase or complex expressions are passed through verbatim.
    fn component_attr_expr(&self, attr: &Attr) -> String {
        if attr.is_quoted {
            return rust_str(&attr.value);
        }
        let v = attr.value.trim();
        if v.starts_with('#') {
            return hex_to_color_expr(v);
        }
        if let Ok(n) = v.parse::<f32>() {
            return format_f32(n);
        }
        let snake = to_snake_case(v);
        let in_style = self
            .constants
            .iter()
            .any(|c| to_snake_case(&c.name) == snake);
        let looks_like_color_name = is_ident(v)
            && v.chars()
                .next()
                .is_some_and(|c| c.is_ascii_lowercase() || c == '_');
        if in_style || (self.theme_type.is_some() && looks_like_color_name) {
            return self.color_expr(v);
        }
        // Verbatim pass-through: tag the value with its source span so the analyzer can complete in it.
        let lead = attr.value.len() - attr.value.trim_start().len();
        format!("{}{v}", expr_marker(attr.value_start + lead, v.len()))
    }

    pub(super) fn emit_widget_ref(&mut self, el: &Element) -> ChildEmit {
        let var = el.content.as_deref().unwrap_or("").trim().to_string();
        ChildEmit::Simple {
            name: var,
            code: String::new(),
        }
    }
}
