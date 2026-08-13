//! Media emitters: `img`/`image` and `svg`.

use telar_parser::{Attr, Element};

use super::signals::{rust_str, substitute_reads, wrap_signal_clones};
use super::{ChildEmit, ViewGen, expr_marker};

/// Parses the shared `fit:` attribute (CSS `object-fit`) for `img`/`svg` into a reactive `ObjectFit` closure. Absent or unrecognized values default to `Contain` (preserve aspect ratio, letterbox), matching the widget defaults.
fn fit_closure(attributes: &[Attr]) -> &'static str {
    match attributes.iter().find(|a| a.key == "fit") {
        Some(a) => match a.value.trim().to_ascii_lowercase().as_str() {
            "fill" => "move || ObjectFit::Fill",
            "cover" => "move || ObjectFit::Cover",
            "contain-integer" => "move || ObjectFit::ContainInteger",
            _ => "move || ObjectFit::Contain",
        },
        None => "move || ObjectFit::Contain",
    }
}

/// Which media widget a `src` binding is for; selects the runtime data type, the baked-asset `static` prefix, the missing-`src` placeholder identifier, and the baker used at build time.
#[derive(Clone, Copy)]
enum MediaKind {
    Svg,
    Image,
}

impl MediaKind {
    fn placeholder(self) -> &'static str {
        match self {
            MediaKind::Svg => "__svg_data",
            MediaKind::Image => "__img_data",
        }
    }

    fn label(self) -> &'static str {
        match self {
            MediaKind::Svg => "SVG",
            MediaKind::Image => "image",
        }
    }

    fn data_ty(self) -> &'static str {
        match self {
            MediaKind::Svg => "SvgData",
            MediaKind::Image => "ImageData",
        }
    }

    fn static_prefix(self) -> &'static str {
        match self {
            MediaKind::Svg => "BAKED_SVG",
            MediaKind::Image => "BAKED_IMG",
        }
    }
}

impl ViewGen<'_> {
    pub(super) fn emit_image(&mut self, el: &Element) -> ChildEmit {
        let var = self.next_variable_name("img");
        let pad = self.indent_str();

        let (setup, data_fn) = self.media_src_binding(
            el.attributes.iter().find(|a| a.key == "src"),
            MediaKind::Image,
        );

        let filter = el
            .attributes
            .iter()
            .find(|a| a.key == "filter")
            .map(|a| match a.value.trim() {
                "Nearest" | "nearest" => "ImageFilter::Nearest",
                _ => "ImageFilter::Linear",
            })
            .unwrap_or("ImageFilter::Linear");

        let fit = fit_closure(&el.attributes);

        let layout_style = self.make_layout_style("img", &el.classes, &el.attributes);

        // Rounding is the picture's own, not a parent's: clipping to a rounded box would need a container per
        // image, and a thumbnail grid is where that cost lands hardest. Resolved by the same helper a `box`
        // uses, so a picture takes the per-corner form and theme tokens on the same terms rather than on the
        // narrower ones this call site used to allow.
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
             {pad}        move || {filter},\n\
             {pad}        {fit},\n\
             {pad}    )?{radius}\n\
             {pad}}};"
        );

        ChildEmit::Simple { name: var, code }
    }

    /// Mirrors `emit_image`: the `src` resolves either to a build-time-baked static asset (quoted `src:"path"`) or a verbatim `Arc<SvgData>` expression (dynamic). `tint` is optional and, unlike `src`, is embedded directly in its closure since a `Color` is cheap to recompute per call.
    pub(super) fn emit_svg(&mut self, el: &Element) -> ChildEmit {
        let var = self.next_variable_name("svg");
        let pad = self.indent_str();

        let (setup, data_fn) = self.media_src_binding(
            el.attributes.iter().find(|a| a.key == "src"),
            MediaKind::Svg,
        );

        let tint_fn = self.svg_tint_closure(el.attributes.iter().find(|a| a.key == "tint"));
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
             {pad}        {tint_fn},\n\
             {pad}        {fit},\n\
             {pad}    )?{stroke_call}\n\
             {pad}}};"
        );

        ChildEmit::Simple { name: var, code }
    }

    /// Resolves `tint:` into a `move || Option<Color>` closure, sharing `fill`/`stroke`/`color`'s
    /// [`color_expr`](ViewGen::color_expr) resolution: a bare theme token (`tint:accent`), a `$signal`
    /// read, an inline `#hex`, a CSS keyword, and an arbitrary color expression (`tint:theme().primary`,
    /// recognized by its `(`) all resolve identically — so an icon tints from a theme token the same way
    /// text takes `color:`. A token re-reads `use_theme` on every `view()`, so a runtime theme switch
    /// recolors the glyph; any `$signal` referenced is cloned into the closure via `wrap_signal_clones`
    /// so the outer handle stays usable, mirroring `box fill:$sig`. Missing or empty `tint` keeps the
    /// SVG's own colors (`None`).
    fn svg_tint_closure(&self, tint_attr: Option<&Attr>) -> String {
        let Some(a) = tint_attr else {
            return "|| None".to_string();
        };
        let v = a.value.trim();
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
        let v = a.value.trim();
        if v.is_empty() {
            return "|| None".to_string();
        }
        let expr = substitute_reads(&crate::style::format_number(v, self.theme_type.as_deref()));
        // `.into()` rather than `Some(…)`, so both a width and an already-optional one work: std gives
        // `From<T> for Option<T>` and the identity `From<T> for T`, and `with_stroke`'s parameter fixes the
        // target. A theme that carries "no override" as `None` can then be passed straight through.
        wrap_signal_clones(&[v], format!("move || ({expr}).into()"))
    }

    /// Resolves a media widget's `src` attribute into `(setup, data_fn)` fragments that slot into its construction block.
    ///
    /// - Quoted, non-empty `src:"path"` is a static asset baked at build time: `setup` declares a `static LazyLock<Arc<Data>>` built once, `data_fn` clones the shared `Arc` per reactive call.
    /// - Non-quoted `src:$signal` (or any expression referencing a `$signal`) is a *reactive* handle: `data_fn` re-reads it on every `view()` so the glyph/image swaps when the bound state changes — the path adaptive icons need (a battery/wifi glyph that tracks its level). Signals are cloned into the closure via `wrap_signal_clones` so the outer handle stays usable, mirroring `svg tint:$sig` / `box fill:$sig`.
    /// - Non-quoted, `$`-free `src:expr` is a constant `Arc<Data>` handle: `setup` hoists it into `__src` once and `data_fn` clones the (cheap) handle. The verbatim span marker is preserved so the analyzer can resolve/rename the symbol inside `expr`.
    /// - Missing or empty `src` falls back to an undefined placeholder identifier, so rustc's "cannot find value" error lands on this `.rsx` line via the source map.
    fn media_src_binding(&mut self, src_attr: Option<&Attr>, kind: MediaKind) -> (String, String) {
        let pad = self.indent_str();
        match src_attr {
            Some(a) if a.is_quoted && !a.value.trim().is_empty() => {
                self.bake_asset_binding(a.value.trim(), kind, &pad)
            }
            Some(a) if !a.is_quoted && a.value.contains('$') && !a.value.trim().is_empty() => {
                let v = a.value.trim();
                let data_fn = wrap_signal_clones(&[v], format!("move || {}", substitute_reads(v)));
                (String::new(), data_fn)
            }
            Some(a) if !a.is_quoted && !a.value.trim().is_empty() => {
                let v = a.value.trim();
                let lead = a.value.len() - a.value.trim_start().len();
                let src = format!("{}{v}", expr_marker(a.value_start + lead, v.len()));
                (
                    format!("{pad}    let __src = {src}.clone();\n"),
                    "move || __src.clone()".to_string(),
                )
            }
            _ => (
                format!("{pad}    let __src = {}.clone();\n", kind.placeholder()),
                "move || __src.clone()".to_string(),
            ),
        }
    }

    /// Bakes the static asset at `rel` (relative to the `.rsx`'s directory) into a shared `static LazyLock<Arc<Data>>` and returns its `(setup, data_fn)`. A read/parse/decode failure becomes a `compile_error!` in the `data_fn` closure, whose `!`-typed body unifies with the widget's `Fn() -> Arc<Data>` bound so no secondary type errors leak.
    fn bake_asset_binding(&mut self, rel: &str, kind: MediaKind, pad: &str) -> (String, String) {
        let expr = match self.bake_asset_expr(rel, kind) {
            Ok(expr) => expr,
            Err(msg) => {
                return (
                    String::new(),
                    format!("move || compile_error!({})", rust_str(&msg)),
                );
            }
        };
        let n = self.baked_asset_count;
        self.baked_asset_count += 1;
        let static_name = format!("{}_{n}", kind.static_prefix());
        let data_ty = kind.data_ty();
        let setup = format!(
            "{pad}    static {static_name}: std::sync::LazyLock<std::sync::Arc<{data_ty}>> = std::sync::LazyLock::new(|| std::sync::Arc::new({expr}));\n"
        );
        let data_fn = format!("move || std::sync::Arc::clone(&{static_name})");
        (setup, data_fn)
    }

    /// Reads and bakes the asset at `rel` into a Rust expression that reconstructs its native data (`SvgData`/`ImageData`), or an error message describing the failed resolution/parse.
    fn bake_asset_expr(&self, rel: &str, kind: MediaKind) -> Result<String, String> {
        let Some(base) = self.base_dir.as_deref() else {
            return Err(format!(
                "rsx: cannot bake {} asset `{rel}`: no base directory is available for this .rsx",
                kind.label()
            ));
        };
        let path = base.join(rel);
        match kind {
            MediaKind::Svg => {
                let content = std::fs::read_to_string(&path).map_err(|e| {
                    format!(
                        "rsx: SVG asset `{rel}` not found at {}: {e}",
                        path.display()
                    )
                })?;
                renderer_assets::bake_to_source(&content)
                    .map_err(|e| format!("rsx: failed to bake SVG asset `{rel}`: {e}"))
            }
            MediaKind::Image => {
                let bytes = std::fs::read(&path).map_err(|e| {
                    format!(
                        "rsx: image asset `{rel}` not found at {}: {e}",
                        path.display()
                    )
                })?;
                renderer_assets::bake_image_to_source(&bytes)
                    .map_err(|e| format!("rsx: failed to bake image asset `{rel}`: {e}"))
            }
        }
    }
}
