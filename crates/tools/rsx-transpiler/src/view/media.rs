//! Media emitters: `img`/`image` and `svg`.

use rsx_parser::Element;

use super::{ChildEmit, ViewGen, expr_marker};

impl ViewGen<'_> {
    pub(super) fn emit_image(&mut self, el: &Element) -> ChildEmit {
        let var = self.next_variable_name("img");
        let pad = self.indent_str();

        // `src` is a verbatim Rust expression (e.g. `gradient_img`). Tag it with its source span so the analyzer can resolve / rename the symbol inside it; quoted values keep the legacy passthrough.
        let src = match el.attributes.iter().find(|a| a.key == "src") {
            Some(a) if !a.is_quoted && !a.value.trim().is_empty() => {
                let v = a.value.trim();
                let lead = a.value.len() - a.value.trim_start().len();
                format!("{}{v}", expr_marker(a.value_start + lead, v.len()))
            }
            Some(a) => a.value.clone(),
            None => "__img_data".to_string(),
        };

        let filter = el
            .attributes
            .iter()
            .find(|a| a.key == "filter")
            .map(|a| match a.value.trim() {
                "Nearest" | "nearest" => "ImageFilter::Nearest",
                _ => "ImageFilter::Linear",
            })
            .unwrap_or("ImageFilter::Linear");

        let layout_style = self.make_layout_style("img", &el.classes, &el.attributes);

        let code = format!(
            "{pad}let {var} = {{\n\
             {pad}    let __src = {src}.clone();\n\
             {pad}    Image::new(\n\
             {pad}        ctx,\n\
             {pad}        {layout_style},\n\
             {pad}        move || __src.clone(),\n\
             {pad}        move || {filter},\n\
             {pad}    )?\n\
             {pad}}};"
        );

        ChildEmit::Simple { name: var, code }
    }

    /// Mirrors `emit_image`: `src` is a verbatim `Arc<SvgData>` expression, hoisted once into `__src` so the reactive closure only clones the (cheap) Arc handle. `tint` is optional and, unlike `src`, is embedded directly in its closure since a `Color` is cheap to recompute per call.
    pub(super) fn emit_svg(&mut self, el: &Element) -> ChildEmit {
        let var = self.next_variable_name("svg");
        let pad = self.indent_str();

        // Same verbatim-with-span handling as `img`'s `src`; a missing attr falls back to an undefined identifier so rustc's error lands on this `.rsx` line via the source map.
        let src = match el.attributes.iter().find(|a| a.key == "src") {
            Some(a) if !a.is_quoted && !a.value.trim().is_empty() => {
                let v = a.value.trim();
                let lead = a.value.len() - a.value.trim_start().len();
                format!("{}{v}", expr_marker(a.value_start + lead, v.len()))
            }
            Some(a) => a.value.clone(),
            None => "__svg_data".to_string(),
        };

        let tint = el.attributes.iter().find(|a| a.key == "tint").map(|a| {
            if !a.is_quoted && !a.value.trim().is_empty() {
                let v = a.value.trim();
                let lead = a.value.len() - a.value.trim_start().len();
                format!("{}{v}", expr_marker(a.value_start + lead, v.len()))
            } else {
                a.value.clone()
            }
        });
        let tint_fn = match tint {
            Some(expr) => format!("move || Some({expr})"),
            None => "|| None".to_string(),
        };

        let layout_style = self.make_layout_style("svg", &el.classes, &el.attributes);

        let code = format!(
            "{pad}let {var} = {{\n\
             {pad}    let __src = {src}.clone();\n\
             {pad}    Svg::new(\n\
             {pad}        ctx,\n\
             {pad}        {layout_style},\n\
             {pad}        move || __src.clone(),\n\
             {pad}        {tint_fn},\n\
             {pad}    )?\n\
             {pad}}};"
        );

        ChildEmit::Simple { name: var, code }
    }
}
