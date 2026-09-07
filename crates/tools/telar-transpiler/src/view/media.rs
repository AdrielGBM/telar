//! Media emitters: `img`/`image` and `svg`.

use telar_parser::{Attr, Element, Value};

use crate::registry;
use telar_project::{AssetContext, AssetKind, asset_kind_for_tag};

use super::signals::{rust_str, substitute_reads, wrap_signal_clones};
use super::{ChildEmit, ViewGen, expr_marker};

/// The shared `fit:` attribute (CSS `object-fit`) for `img`/`svg`, as a reactive `ObjectFit` closure. An absent `fit:` preserves the aspect ratio and letterboxes, matching the widget default.
fn fit_closure(attributes: &[Attr]) -> String {
    let variant = attributes
        .iter()
        .find(|a| a.key == "fit")
        .and_then(|a| registry::keyword(registry::FIT_VALUES, a.value.text().trim()))
        .unwrap_or("ObjectFit::Contain");
    format!("move || {variant}")
}

fn svg_asset() -> &'static AssetKind {
    asset_kind_for_tag("svg").expect("svg is a registered asset kind")
}

fn image_asset() -> &'static AssetKind {
    asset_kind_for_tag("image").expect("image is a registered asset kind")
}

impl ViewGen<'_> {
    pub(super) fn emit_image(&mut self, el: &Element) -> ChildEmit {
        let var = self.next_variable_name(&el.tag);
        let pad = self.indent_str();

        let (setup, data_fn) =
            self.media_src_binding(el.attributes.iter().find(|a| a.key == "src"), image_asset());

        let raster = el
            .attributes
            .iter()
            .find(|a| a.key == "raster")
            .and_then(|a| registry::keyword(registry::RASTER_VALUES, a.value.text().trim()))
            .unwrap_or("Raster::Smooth");

        let fit = fit_closure(&el.attributes);

        let layout_style = self.make_layout_style("img", &el.classes, &el.attributes);

        // The picture's own rounding, not a parent's: clipping to a rounded box would need a container per image, which costs most in a thumbnail grid.
        let rounds = el
            .attributes
            .iter()
            .any(|a| a.key == "radius" || super::signals::is_corner_key(&a.key));
        let radius = if rounds {
            format!(".with_border_radius({})", self.radius_expr(&el.attributes))
        } else {
            String::new()
        };

        let code = format!(
            "{pad}let {var} = {{\n\
             {setup}\
             {pad}    Image::new(\n\
             {pad}        {layout_style},\n\
             {pad}        {data_fn},\n\
             {pad}        move || {raster},\n\
             {pad}        {fit},\n\
             {pad}    )?{radius}\n\
             {pad}}};"
        );

        ChildEmit::Simple { name: var, code }
    }

    /// Mirrors `emit_image`: the `src` resolves either to a build-time-baked static asset (quoted `src:"path"`) or a verbatim `Arc<SvgData>` expression (dynamic). `color` is optional and, unlike `src`, is embedded directly in its closure since a `Color` is cheap to recompute per call.
    pub(super) fn emit_svg(&mut self, el: &Element) -> ChildEmit {
        let var = self.next_variable_name(&el.tag);
        let pad = self.indent_str();

        let (setup, data_fn) =
            self.media_src_binding(el.attributes.iter().find(|a| a.key == "src"), svg_asset());

        let color_fn = self.svg_color_closure(el.attributes.iter().find(|a| a.key == "color"));
        let stroke = el.attributes.iter().find(|a| a.key == "stroke");
        let stroke_call = match stroke {
            Some(_) => format!(".with_stroke({})", self.svg_stroke_closure(stroke)),
            None => String::new(),
        };

        let fit = fit_closure(&el.attributes);

        let layout_style = self.make_layout_style("svg", &el.classes, &el.attributes);

        let code = format!(
            "{pad}let {var} = {{\n\
             {setup}\
             {pad}    Svg::new(\n\
             {pad}        {layout_style},\n\
             {pad}        {data_fn},\n\
             {pad}        {color_fn},\n\
             {pad}        {fit},\n\
             {pad}    )?{stroke_call}\n\
             {pad}}};"
        );

        ChildEmit::Simple { name: var, code }
    }

    /// Resolves `color:` into a `move || Option<Color>` closure, sharing `fill`/`stroke`/`color`'s [`color_expr`](ViewGen::color_expr) resolution: a bare theme token (`color:accent`), a `$signal` read, an inline `#hex`, a CSS keyword, and an arbitrary color expression (`color:theme().primary`, recognized by its `(`) all resolve identically — so an icon takes its ink from a theme token the same way text takes `color:`. A token re-reads `use_theme` on every `view()`, so a runtime theme switch recolors the glyph; any `$signal` referenced is cloned into the closure via `wrap_signal_clones` so the outer handle stays usable, mirroring `box fill:$sig`. Missing or empty `color` keeps the SVG's own colors (`None`).
    fn svg_color_closure(&self, color_attr: Option<&Attr>) -> String {
        let Some(a) = color_attr else {
            return "|| None".to_string();
        };
        let v = a.value.text().trim();
        if v.is_empty() {
            return "|| None".to_string();
        }
        let expr = self.color_expr(v);
        wrap_signal_clones(&[v], format!("move || Some({expr})"))
    }

    /// `stroke:` on an `svg`, overriding the stroke width the document declares. A theme that draws its icons at one weight sets it here rather than editing every asset, which is why the override exists on `Svg` at all — it was simply unreachable from `[view]`.
    fn svg_stroke_closure(&self, stroke_attr: Option<&Attr>) -> String {
        let Some(a) = stroke_attr else {
            return "|| None".to_string();
        };
        let v = a.value.text().trim();
        if v.is_empty() {
            return "|| None".to_string();
        }
        let expr = substitute_reads(&crate::style::number_or(v, "1.0"));
        // `.into()` rather than `Some(…)`, so a width and an already-optional one both work — std gives `From<T> for Option<T>` and the identity, and `with_stroke`'s parameter fixes the target.
        wrap_signal_clones(&[v], format!("move || ({expr}).into()"))
    }

    /// Resolves a media widget's `src` attribute into `(setup, data_fn)` fragments that slot into its construction block.
    ///
    /// - Quoted, non-empty `src:"path"` is a static asset the CLI baked: `data_fn` clones the `Arc` the artifact's `static` holds, and `setup` is empty — the payload lives once per crate in the generated module, not once per use site as it did when the macro baked it.
    /// - Non-quoted `src:$signal` (or any expression referencing a `$signal`) is a *reactive* handle: `data_fn` re-reads it on every `view()` so the glyph/image swaps when the bound state changes — the path adaptive icons need (a battery/wifi glyph that tracks its level). Signals are cloned into the closure via `wrap_signal_clones` so the outer handle stays usable, mirroring `svg color:$sig` / `box fill:$sig`.
    /// - Non-quoted, `$`-free `src:expr` is a constant `Arc<Data>` handle: `setup` hoists it into `__src` once and `data_fn` clones the (cheap) handle. The verbatim span marker is preserved so the analyzer can resolve/rename the symbol inside `expr`.
    /// - Missing, empty, or written in a form that cannot name an asset (a bare flag, a `t"…"` key) falls back to an undefined placeholder identifier, so rustc's "cannot find value" error lands on this `.rsx` line via the source map.
    fn media_src_binding(
        &self,
        src_attr: Option<&Attr>,
        kind: &'static AssetKind,
    ) -> (String, String) {
        let pad = self.indent_str();
        match src_attr.map(|a| (a, &a.value)) {
            Some((_, Value::Quoted(path))) if !path.trim().is_empty() => {
                (String::new(), self.static_asset_data_fn(path.trim(), kind))
            }
            Some((a, Value::Expr(expr) | Value::Directive(expr))) if !expr.trim().is_empty() => {
                let v = expr.trim();
                if v.contains('$') {
                    let data_fn =
                        wrap_signal_clones(&[v], format!("move || {}", substitute_reads(v)));
                    return (String::new(), data_fn);
                }
                let lead = expr.len() - expr.trim_start().len();
                let src = format!("{}{v}", expr_marker(a.value_start + lead, v.len()));
                (
                    format!("{pad}    let __src = {src}.clone();\n"),
                    "move || __src.clone()".to_string(),
                )
            }
            _ => (
                format!("{pad}    let __src = {}.clone();\n", kind.placeholder),
                "move || __src.clone()".to_string(),
            ),
        }
    }

    /// The `data_fn` for a static `src:"rel"`: a clone of the `static` the baked artifact declares for it. Nothing is read or decoded here — an asset the artifact cannot answer for becomes a `compile_error!` naming the command that bakes it, and that call's `!`-typed result unifies with the widget's `Fn() -> Arc<Data>` bound, so no secondary type errors leak from it.
    fn static_asset_data_fn(&self, rel: &str, kind: &'static AssetKind) -> String {
        let resolved = match self.assets {
            Some(assets) => assets.resolve(kind, rel),
            None => Err(AssetContext::detached_message(kind, rel)),
        };
        match resolved {
            Ok(path) => format!("move || std::sync::Arc::clone(&{path})"),
            Err(msg) => format!("move || compile_error!({})", rust_str(&msg)),
        }
    }
}
