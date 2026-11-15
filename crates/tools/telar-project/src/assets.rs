//! Registry of external asset kinds a `.rsx` element may reference: by path through a built-in tag's `src:"path"`, or by id through a component's prop, such as `icon name:"mdi:home"`.
//!
//! The single source of truth for which tags carry an asset reference and what goes with each: the runtime data type, the baked-asset `static` prefix, the missing-`src` placeholder identifier, and the file extensions that name it. `telar-analyzer`'s document links and `cargo-telar`'s asset watcher ask [`AssetKind`] instead of keeping their own copy.

mod artifact;
mod context;

pub use artifact::{
    ASSET_ARTIFACT_FORMAT, ASSETS_INDEX_FILENAME, ASSETS_MODULE, ASSETS_SOURCE_FILENAME,
    ArtifactHandshake, AssetEntry, AssetIndex, BakedAsset, GeneratedAssets, baked_init_expr,
    check_artifact, content_hash, generate_assets, read_index, static_name_for_path,
    write_generated,
};
pub use context::{AssetContext, BakedId};

use serde::{Deserialize, Serialize};

/// One kind of external asset a `.rsx` element can name: by path through a built-in tag's `src:`, or by id through a component's prop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AssetKind {
    Svg,
    Image,
    Icon,
}

/// Text that names no [`AssetKind`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownAssetKind(pub String);

impl std::fmt::Display for UnknownAssetKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kinds: Vec<&str> = AssetKind::ALL.into_iter().map(AssetKind::id).collect();
        write!(
            f,
            "`{}` is not an asset kind: one of {}",
            self.0,
            kinds.join(", ")
        )
    }
}

impl std::error::Error for UnknownAssetKind {}

impl AssetKind {
    pub const ALL: [AssetKind; 3] = [AssetKind::Svg, AssetKind::Image, AssetKind::Icon];

    /// The kind `id` names, as [`Self::id`] spells it.
    pub fn parse(id: &str) -> Result<Self, UnknownAssetKind> {
        Self::ALL
            .into_iter()
            .find(|kind| kind.id() == id)
            .ok_or_else(|| UnknownAssetKind(id.to_string()))
    }

    /// The kind the built-in `tag` resolves to, or `None` for a tag that carries no asset path.
    pub fn for_tag(tag: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|kind| kind.tags().contains(&tag))
    }

    /// The component-named kind whose id `prop` carries on the component `tag`, whether or not the package bakes it: the kind's own component's prop, or one of its [`ComponentAsset::carriers`].
    pub fn for_component(tag: &str, prop: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| {
            kind.component()
                .is_some_and(|component| component.takes_id(kind.attr(), tag, prop))
        })
    }

    /// Short identifier for the kind, independent of any tag spelling: what an artifact records it as.
    pub fn id(self) -> &'static str {
        match self {
            Self::Svg => "svg",
            Self::Image => "image",
            Self::Icon => "icon",
        }
    }

    /// Every tag spelling that resolves to this kind (`img`/`image` share one).
    pub fn tags(self) -> &'static [&'static str] {
        match self {
            Self::Svg => &["svg"],
            Self::Image => &["img", "image"],
            Self::Icon => &[],
        }
    }

    /// The attribute carrying the asset path, or for a component-named kind, its id.
    pub fn attr(self) -> &'static str {
        match self {
            Self::Svg | Self::Image => "src",
            Self::Icon => "name",
        }
    }

    /// The runtime type `src:` produces, wrapped in `Arc<..>`.
    pub fn data_ty(self) -> &'static str {
        match self {
            Self::Svg | Self::Icon => "SvgData",
            Self::Image => "ImageData",
        }
    }

    /// The identifier substituted in for a `src:` that is missing, empty, or written in a form that cannot name an asset, so rustc's error lands on the right `.rsx` line via the source map.
    pub fn placeholder(self) -> &'static str {
        match self {
            Self::Svg => "__svg_data",
            Self::Image => "__img_data",
            Self::Icon => "__icon_data",
        }
    }

    /// The `static` name prefix for a baked instance of this kind, so each one gets a unique `BAKED_*_N` binding.
    pub fn static_prefix(self) -> &'static str {
        match self {
            Self::Svg => "BAKED_SVG",
            Self::Image => "BAKED_IMG",
            Self::Icon => "BAKED_ICON",
        }
    }

    /// The `__<prefix>_N` local variable name codegen assigns to a widget of this kind, shared by every tag spelling in [`Self::tags`].
    pub fn var_prefix(self) -> &'static str {
        match self {
            Self::Svg => "svg",
            Self::Image => "img",
            Self::Icon => "icon",
        }
    }

    /// Human-readable name for diagnostics (e.g. "cannot bake {label} asset").
    pub fn label(self) -> &'static str {
        match self {
            Self::Svg => "SVG",
            Self::Image => "image",
            Self::Icon => "icon",
        }
    }

    /// File extensions that name this kind, without the leading dot.
    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            Self::Svg => &["svg"],
            // Every format the CLI's baker decodes, not the subset an app's own runtime does — this is what a watcher asks "is that file an asset", and a short list means editing a `.webp` raises no event. `telar-baker`'s `registry_lists_every_readable_format` holds the two in step; the transpiler cannot ask `image` itself without linking it.
            Self::Image => &[
                "avif", "bmp", "exr", "ff", "gif", "hdr", "ico", "jpeg", "jpg", "pam", "pbm",
                "pgm", "png", "pnm", "ppm", "qoi", "tga", "tif", "tiff", "webp",
            ],
            Self::Icon => &[],
        }
    }

    /// For a kind named by id through a component's prop rather than by path through a built-in tag. `None` for a path asset.
    pub fn component(self) -> Option<ComponentAsset> {
        match self {
            Self::Svg | Self::Image => None,
            Self::Icon => Some(ComponentAsset {
                tag: "icon",
                section: "icons",
                carriers: &[IdProp {
                    tag: "icon_button",
                    prop: "icon",
                }],
            }),
        }
    }
}

/// Where a component-named kind is written and configured. The id is the literal value of [`AssetKind::attr`] on [`Self::tag`]; the baker resolves it through the sources `[telar.<section>]` names, and in a package that bakes it the transpiler hands the prop `(id, Arc<data>, monochrome)` instead of the bare string, which the component's prop type accepts through `From`; `monochrome` says whether the artwork takes the colour around it (see [`BakedAsset::monochrome`](crate::BakedAsset::monochrome)). The id is spelled the way the bake keys it, so a bare icon name arrives read in its default set.
///
/// The general half of this is the protocol — a tag, a prop, a section, and the pair the prop receives. What cannot be general is the resolver: it runs inside the CLI at bake time, and the CLI does not load plugin code, so each kind is an entry here plus a resolver in `telar-baker`, the same as a path kind is an entry plus a `Baker`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComponentAsset {
    pub tag: &'static str,
    /// The `[telar.<section>]` table that configures this kind, as a message names it.
    pub section: &'static str,
    /// The props of other components that take an id of this kind and draw it through [`Self::tag`], such as `icon_button icon:"lucide:x"`. Listed here because the bake reads `.rsx` before anything is compiled, so a prop's type cannot say it; one left out reaches its component as the bare id, which a baked-only package cannot draw.
    ///
    /// Only a literal given to a carrier is an id: anything else is left as written, because a carrier also takes what the component's own macro builds, `icon_button icon:icon!("lucide:x")`, whose id the bake reads from the Rust.
    pub carriers: &'static [IdProp],
}

/// A component's prop that takes an id of a component-named kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdProp {
    pub tag: &'static str,
    pub prop: &'static str,
}

impl ComponentAsset {
    /// Whether `prop` on `tag` is one of [`Self::carriers`].
    pub fn carries(&self, tag: &str, prop: &str) -> bool {
        self.carriers
            .iter()
            .any(|carrier| carrier.tag == tag && carrier.prop == prop)
    }

    /// Whether `prop` on `tag` takes an id of this kind, given the attribute its own component takes it by.
    fn takes_id(&self, attr: &str, tag: &str, prop: &str) -> bool {
        (self.tag == tag && attr == prop) || self.carries(tag, prop)
    }
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
