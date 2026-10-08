//! Registry of external asset kinds a `.rsx` element may reference: by path through a built-in tag's `src:"path"`, or by id through a component's prop, such as `icon name:"mdi:home"`.
//!
//! The single source of truth for which tags carry an asset reference and what goes with each: the runtime data type, the baked-asset `static` prefix, the missing-`src` placeholder identifier, and the file extensions that name it. `telar-analyzer`'s document links and `cargo-telar`'s asset watcher read this table instead of keeping their own copy.

mod artifact;
mod context;

pub use artifact::{
    ASSET_ARTIFACT_FORMAT, ASSETS_INDEX_FILENAME, ASSETS_MODULE, ASSETS_SOURCE_FILENAME,
    ArtifactHandshake, AssetEntry, AssetIndex, BakedAsset, GeneratedAssets, baked_init_expr,
    check_artifact, content_hash, generate_assets, read_index, static_name_for_path,
    write_generated,
};
pub use context::{AssetContext, BakedId};

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
    /// The `__<prefix>_N` local variable name codegen assigns to a widget of this kind, shared by every tag spelling in [`Self::tags`].
    pub var_prefix: &'static str,
    /// Human-readable name for diagnostics (e.g. "cannot bake {label} asset").
    pub label: &'static str,
    /// File extensions that name this kind, without the leading dot.
    pub extensions: &'static [&'static str],
    /// For a kind named by id through a component's prop rather than by path through a built-in tag. `None` for a path asset.
    pub component: Option<ComponentAsset>,
}

/// Where a component-named kind is written and configured. The id is the literal value of [`AssetKind::attr`] on [`Self::tag`]; the baker resolves it through the sources `[telar.<section>]` names, and in a package that bakes it the transpiler hands the prop `(id, Arc<data>, monochrome)` instead of the bare string, which the component's prop type accepts through `From`; `monochrome` says whether the artwork takes the colour around it (see [`BakedAsset::monochrome`](crate::BakedAsset::monochrome)). The id is spelled the way the bake keys it, so a bare icon name arrives read in its default set.
///
/// The general half of this is the protocol — a tag, a prop, a section, and the pair the prop receives. What cannot be general is the resolver: it runs inside the CLI at bake time, and the CLI does not load plugin code, so each kind is an entry here plus a resolver in `telar-baker`, the same as a path kind is an entry plus a `Baker`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComponentAsset {
    pub tag: &'static str,
    /// The `[telar.<section>]` table that configures this kind, read by [`crate::TelarSection::id_baking`].
    pub section: &'static str,
}

/// What a package does with the ids its `.rsx` gives a component-named kind's prop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdBaking {
    /// Nothing is baked; every id reaches the component as written, for a runtime source to resolve.
    Off,
    /// Literal ids are baked, and any other reaches the component as written.
    Literals,
    /// Every id is baked, so one that is not a literal is an error.
    Required,
}

pub const ASSET_KINDS: &[AssetKind] = &[
    AssetKind {
        id: "svg",
        tags: &["svg"],
        attr: "src",
        data_ty: "SvgData",
        placeholder: "__svg_data",
        static_prefix: "BAKED_SVG",
        var_prefix: "svg",
        label: "SVG",
        extensions: &["svg"],
        component: None,
    },
    AssetKind {
        id: "image",
        tags: &["img", "image"],
        attr: "src",
        data_ty: "ImageData",
        placeholder: "__img_data",
        static_prefix: "BAKED_IMG",
        var_prefix: "img",
        label: "image",
        // Every format the CLI's baker decodes, not the subset an app's own runtime does — this is what a watcher asks "is that file an asset", and a short list means editing a `.webp` raises no event. `telar-baker`'s `registry_lists_every_readable_format` holds the two in step; the transpiler cannot ask `image` itself without linking it.
        extensions: &[
            "avif", "bmp", "exr", "ff", "gif", "hdr", "ico", "jpeg", "jpg", "pam", "pbm", "pgm",
            "png", "pnm", "ppm", "qoi", "tga", "tif", "tiff", "webp",
        ],
        component: None,
    },
    AssetKind {
        id: "icon",
        tags: &[],
        attr: "name",
        data_ty: "SvgData",
        placeholder: "__icon_data",
        static_prefix: "BAKED_ICON",
        var_prefix: "icon",
        label: "icon",
        extensions: &[],
        component: Some(ComponentAsset {
            tag: "icon",
            section: "icons",
        }),
    },
];

/// The asset kind the built-in `tag` resolves to, or `None` for a tag that carries no asset path.
pub fn asset_kind_for_tag(tag: &str) -> Option<&'static AssetKind> {
    ASSET_KINDS.iter().find(|kind| kind.tags.contains(&tag))
}

/// The component-named kind whose id `prop` carries on the component `tag`, whether or not the package bakes it.
pub fn asset_kind_for_component(tag: &str, prop: &str) -> Option<&'static AssetKind> {
    ASSET_KINDS
        .iter()
        .find(|kind| kind.attr == prop && kind.component.is_some_and(|c| c.tag == tag))
}

/// The asset kind identified by [`AssetKind::id`], or `None` if `id` names no registered kind.
pub fn asset_kind_for_id(id: &str) -> Option<&'static AssetKind> {
    ASSET_KINDS.iter().find(|kind| kind.id == id)
}
