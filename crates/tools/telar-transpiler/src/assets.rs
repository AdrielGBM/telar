//! Registry of external asset kinds a `.rsx` element may reference via `src:"path"`.
//!
//! The single source of truth for which built-in tags carry an asset reference and what goes with each: the runtime data type, the baked-asset `static` prefix, the missing-`src` placeholder identifier, and the file extensions that name it. `telar-analyzer`'s document links and `cargo-telar`'s asset watcher read this table instead of keeping their own copy.


/// One kind of external asset a widget's `src:` attribute can resolve to.
pub struct AssetKind {
    /// Short identifier for the kind, independent of any tag spelling.
    pub id: &'static str,
    /// Every tag spelling that resolves to this kind (`img`/`image` share one).
    pub tags: &'static [&'static str],
    /// The attribute carrying the asset path.
    pub attr: &'static str,
    /// The runtime type `src:` produces, wrapped in `Arc<..>`.
    pub data_ty: &'static str,
    /// The identifier substituted in for a `src:` that is missing, empty, or written in a form that cannot name an asset, so rustc's error lands on the right `.rsx` line via the source map.
    pub placeholder: &'static str,
    /// The `static` name prefix for a baked instance of this kind, so each one gets a unique `BAKED_*_N` binding.
    pub static_prefix: &'static str,
    /// Human-readable name for diagnostics (e.g. "cannot bake {label} asset").
    pub label: &'static str,
    /// File extensions that name this kind, without the leading dot.
    pub extensions: &'static [&'static str],
}

pub const ASSET_KINDS: &[AssetKind] = &[
    AssetKind {
        id: "svg",
        tags: &["svg"],
        attr: "src",
        data_ty: "SvgData",
        placeholder: "__svg_data",
        static_prefix: "BAKED_SVG",
        label: "SVG",
        extensions: &["svg"],
    },
    AssetKind {
        id: "image",
        tags: &["img", "image"],
        attr: "src",
        data_ty: "ImageData",
        placeholder: "__img_data",
        static_prefix: "BAKED_IMG",
        label: "image",
        extensions: &["png", "jpg", "jpeg"],
    },
];

/// The asset kind `tag` resolves to, or `None` for a tag that carries no asset reference.
pub fn asset_kind_for_tag(tag: &str) -> Option<&'static AssetKind> {
    ASSET_KINDS.iter().find(|kind| kind.tags.contains(&tag))
}
