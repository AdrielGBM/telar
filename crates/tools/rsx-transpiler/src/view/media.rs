//! Media emitters: `img`/`image` and `svg`.

use rsx_parser::{Attr, Element};

use super::signals::rust_str;
use super::{ChildEmit, ViewGen, expr_marker};

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

        let layout_style = self.make_layout_style("img", &el.classes, &el.attributes);

        let code = format!(
            "{pad}let {var} = {{\n\
             {setup}\
             {pad}    Image::new(\n\
             {pad}        ctx,\n\
             {pad}        {layout_style},\n\
             {pad}        {data_fn},\n\
             {pad}        move || {filter},\n\
             {pad}    )?\n\
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
             {setup}\
             {pad}    Svg::new(\n\
             {pad}        ctx,\n\
             {pad}        {layout_style},\n\
             {pad}        {data_fn},\n\
             {pad}        {tint_fn},\n\
             {pad}    )?\n\
             {pad}}};"
        );

        ChildEmit::Simple { name: var, code }
    }

    /// Resolves a media widget's `src` attribute into `(setup, data_fn)` fragments that slot into its construction block.
    ///
    /// - Quoted, non-empty `src:"path"` is a static asset baked at build time: `setup` declares a `static LazyLock<Arc<Data>>` built once, `data_fn` clones the shared `Arc` per reactive call.
    /// - Non-quoted, non-empty `src:expr` is a dynamic `Arc<Data>` expression: `setup` hoists it into `__src` and `data_fn` clones the (cheap) handle. The verbatim span marker is preserved so the analyzer can resolve/rename the symbol inside `expr`.
    /// - Missing or empty `src` falls back to an undefined placeholder identifier, so rustc's "cannot find value" error lands on this `.rsx` line via the source map.
    fn media_src_binding(&mut self, src_attr: Option<&Attr>, kind: MediaKind) -> (String, String) {
        let pad = self.indent_str();
        match src_attr {
            Some(a) if a.is_quoted && !a.value.trim().is_empty() => {
                self.bake_asset_binding(a.value.trim(), kind, &pad)
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
