//! [`SvgData`]: an SVG as either a vector display list or a rasterized fallback, fitted per use.

#[cfg(feature = "dynamic-svg")]
mod bake;
#[cfg(feature = "dynamic-svg")]
mod raster;
mod refit;
#[cfg(all(test, feature = "dynamic-svg"))]
mod tests;
#[cfg(feature = "dynamic-svg")]
mod vector;

use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use geometry_core::{ObjectFit, Point};
use rustc_hash::{FxHashMap, FxHasher};

use renderer_core::{
    Color, DrawCommand, ImageData, PathData, PathStyle, PathVerb, hash_path_style,
};

#[cfg(feature = "dynamic-svg")]
use usvg::tiny_skia_path::Transform as SkiaTransform;
#[cfg(feature = "dynamic-svg")]
use vector::convert_group;

#[cfg(feature = "dynamic-svg")]
pub use bake::bake_to_source;

#[derive(Debug, Clone, thiserror::Error)]
#[error("failed to parse SVG: {0}")]
/// An SVG could not be parsed or converted.
pub struct SvgError(String);

// Memo key: (width bits, height bits, tint as packed rgba bits or None, stroke-width override bits or None, object-fit discriminant). f32 goes through `to_bits` so it is Eq/Hash.
type MemoKey = (u32, u32, Option<[u32; 4]>, Option<u32>, u8);

/// An SVG document that converts to the renderer's `DrawCommand`s.
///
/// It is either parsed at runtime with usvg (the `dynamic-svg` path) or BAKED at build time into a display list that renders with no SVG dependency. The vector case is the common one; SVGs using features without a drawing primitive fall back to a single rasterized `DrawCommand::Image`.
pub struct SvgData {
    // Content-derived (not a counter) so rebuilding the same SVG keeps a stable id for downstream caches.
    id: u64,
    size: (f32, f32),
    source: SvgSource,
    memo: Mutex<FxHashMap<MemoKey, Arc<Vec<DrawCommand>>>>,
}

enum SvgSource {
    #[cfg(feature = "dynamic-svg")]
    Parsed(usvg::Tree),
    Baked(BakedSvg),
}

/// A pre-converted SVG in intrinsic viewBox space, with original colors and no letterbox fit applied. One command of a baked vector list: exactly the three shapes `vector::convert_group` produces, and no others.
///
/// A `Vec<DrawCommand>` said the same thing in a comment, and three walkers over it each answered a different way when handed something else — one panicked, one cloned it through with a debug assertion, one hashed a marker byte — while the public constructor accepted any command at all. Narrowing the type deletes the question: every walk is exhaustive and there is no unexpected command to disagree about.
#[derive(Debug, Clone, PartialEq)]
pub enum VectorCommand {
    Path {
        data: Arc<PathData>,
        style: Arc<PathStyle>,
    },
    PushLayer {
        opacity: f32,
        backdrop_blur: f32,
    },
    PopLayer,
}

impl VectorCommand {
    /// The renderer's own command for this one. Total in this direction — every vector command is a draw command, which is why the narrowing costs nothing at the point of use.
    pub(crate) fn to_draw(&self) -> DrawCommand {
        match self {
            VectorCommand::Path { data, style } => DrawCommand::Path {
                data: Arc::clone(data),
                style: Arc::clone(style),
            },
            VectorCommand::PushLayer {
                opacity,
                backdrop_blur,
            } => DrawCommand::PushLayer {
                opacity: *opacity,
                backdrop_blur: *backdrop_blur,
            },
            VectorCommand::PopLayer => DrawCommand::PopLayer,
        }
    }
}

pub(crate) enum BakedSvg {
    // Paths plus PushLayer/PopLayer, exactly what `vector::convert_group` produces under an identity transform.
    Vector(Vec<VectorCommand>),
    // The whole SVG pre-rasterized (the fallback for unsupported features), at its own resolution.
    Raster {
        image: Arc<ImageData>,
        raster_size: (f32, f32),
    },
}

// Marks conversion of a feature we have no primitive for; the caller then rasterizes the whole tree.
#[cfg(feature = "dynamic-svg")]
pub(crate) struct Unsupported;

// SvgData is shared across threads via Arc; hold this for every feature combination.
const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<SvgData>();
};

impl SvgData {
    // Named `from_str` per the public API; it is fallible on content, not a `FromStr` impl.
    #[cfg(feature = "dynamic-svg")]
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(svg: &str) -> Result<Self, SvgError> {
        let opt = usvg::Options::default();
        let tree = usvg::Tree::from_str(svg, &opt).map_err(|e| SvgError(e.to_string()))?;
        let size = tree.size();
        let mut hasher = FxHasher::default();
        svg.as_bytes().hash(&mut hasher);
        Ok(Self {
            id: hasher.finish(),
            size: (size.width(), size.height()),
            source: SvgSource::Parsed(tree),
            memo: Mutex::new(FxHashMap::default()),
        })
    }

    /// A pre-baked vector display list (paths plus PushLayer/PopLayer) in intrinsic viewBox space, with original colors and no fit applied. `commands_for` re-fits it into widget space.
    pub fn from_baked_vector(intrinsic: (f32, f32), commands: Vec<VectorCommand>) -> Self {
        Self::from_baked(intrinsic, BakedSvg::Vector(commands))
    }

    /// A pre-rasterized SVG (the fallback for features without a vector primitive). `raster_size` is the bitmap's own resolution; `commands_for` draws it into the letterboxed content rect.
    pub fn from_baked_raster(
        intrinsic: (f32, f32),
        image: ImageData,
        raster_size: (f32, f32),
    ) -> Self {
        Self::from_baked(
            intrinsic,
            BakedSvg::Raster {
                image: Arc::new(image),
                raster_size,
            },
        )
    }

    pub(crate) fn from_baked(intrinsic: (f32, f32), baked: BakedSvg) -> Self {
        Self {
            id: hash_baked(intrinsic, &baked),
            size: intrinsic,
            source: SvgSource::Baked(baked),
            memo: Mutex::new(FxHashMap::default()),
        }
    }

    /// Stable, content-derived id. Same content produces the same id across instances.
    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn intrinsic_size(&self) -> (f32, f32) {
        self.size
    }

    /// Display list in the widget's LOCAL space (`0,0..width,height`), memoized per `(width, height, tint, stroke, fit)`.
    ///
    /// `stroke` overrides every stroked path's width (in SVG userspace units, e.g. Lucide's `2`) — the theme's icon-stroke token. It applies to the runtime-parsed (`dynamic-svg`) path only; a baked display list keeps its baked widths.
    pub fn commands_for(
        &self,
        width: f32,
        height: f32,
        tint: Option<Color>,
        stroke: Option<f32>,
        fit: ObjectFit,
    ) -> Arc<Vec<DrawCommand>> {
        let key: MemoKey = (
            width.to_bits(),
            height.to_bits(),
            tint.map(|c| [c.r.to_bits(), c.g.to_bits(), c.b.to_bits(), c.a.to_bits()]),
            stroke.map(f32::to_bits),
            fit as u8,
        );
        let mut memo = self.memo.lock().unwrap();
        if let Some(cached) = memo.get(&key) {
            return Arc::clone(cached);
        }
        let commands = Arc::new(self.build_commands(width, height, tint, stroke, fit));
        // Simple bound: drop the whole cache rather than track an LRU; these display lists are cheap to rebuild.
        if memo.len() >= 16 {
            memo.clear();
        }
        memo.insert(key, Arc::clone(&commands));
        commands
    }

    // `stroke` overrides widths only on the parsed path, so it is unused when `dynamic-svg` is off.
    #[cfg_attr(not(feature = "dynamic-svg"), allow(unused_variables))]
    fn build_commands(
        &self,
        width: f32,
        height: f32,
        tint: Option<Color>,
        stroke: Option<f32>,
        fit: ObjectFit,
    ) -> Vec<DrawCommand> {
        let (vb_w, vb_h) = self.size;
        if vb_w <= 0.0 || vb_h <= 0.0 || width <= 0.0 || height <= 0.0 {
            return Vec::new();
        }
        // Shared by both paths so the baked re-fit reproduces the dynamic mapping bit-for-bit (see the equivalence guardrail). `contain`/`cover` give `sx == sy` (uniform, centered); `fill` gives `sx != sy` at the origin.
        let (sx, sy, offset_x, offset_y) = fit_params(vb_w, vb_h, width, height, fit);
        let fitted_w = vb_w * sx;
        let fitted_h = vb_h * sy;

        match &self.source {
            #[cfg(feature = "dynamic-svg")]
            SvgSource::Parsed(tree) => {
                let fit_ts = SkiaTransform::from_row(sx, 0.0, 0.0, sy, offset_x, offset_y);
                let mut out = Vec::new();
                match convert_group(tree.root(), fit_ts, tint, stroke, &mut out) {
                    Ok(()) => out.iter().map(VectorCommand::to_draw).collect(),
                    Err(Unsupported) => raster::raster_fallback(
                        tree, self.size, fitted_w, fitted_h, offset_x, offset_y, tint,
                    ),
                }
            }
            SvgSource::Baked(baked) => match baked {
                // Baking converts under an identity transform, so re-fitting each point by `(p.x*sx+dx, p.y*sy+dy)` reproduces the dynamic `fit_ts.pre_concat(abs)` mapping exactly (the determinant argument makes stroke widths and gradient radii agree too).
                BakedSvg::Vector(cmds) => {
                    refit::refit_vector(cmds, sx, sy, offset_x, offset_y, tint)
                }
                BakedSvg::Raster { image, .. } => {
                    refit::refit_raster(image, fitted_w, fitted_h, offset_x, offset_y, tint)
                }
            },
        }
    }
}

/// Maps the intrinsic viewBox into widget space for `fit`, as `(scale_x, scale_y, offset_x, offset_y)`.
///
/// `contain` and `cover` are uniform (`sx == sy`) and centered — `contain` fits inside (letterbox), `cover` fills and overflows (the widget clips it). `fill` stretches each axis independently to the box, anchored at the origin.
fn fit_params(
    vb_w: f32,
    vb_h: f32,
    width: f32,
    height: f32,
    fit: ObjectFit,
) -> (f32, f32, f32, f32) {
    match fit {
        ObjectFit::Fill => (width / vb_w, height / vb_h, 0.0, 0.0),
        // `ContainInteger` fits like `Contain` here: it exists to keep a bitmap's pixel grid even, and a vector has no grid to keep — flooring the scale would only shrink the drawing for nothing.
        ObjectFit::Contain | ObjectFit::Cover | ObjectFit::ContainInteger => {
            let sx = width / vb_w;
            let sy = height / vb_h;
            let s = if fit == ObjectFit::Cover {
                sx.max(sy)
            } else {
                sx.min(sy)
            };
            (s, s, (width - vb_w * s) * 0.5, (height - vb_h * s) * 0.5)
        }
    }
}

fn hash_baked(intrinsic: (f32, f32), baked: &BakedSvg) -> u64 {
    let mut h = FxHasher::default();
    intrinsic.0.to_bits().hash(&mut h);
    intrinsic.1.to_bits().hash(&mut h);
    match baked {
        BakedSvg::Vector(cmds) => {
            0u8.hash(&mut h);
            cmds.len().hash(&mut h);
            for cmd in cmds {
                hash_command(cmd, &mut h);
            }
        }
        BakedSvg::Raster { image, raster_size } => {
            1u8.hash(&mut h);
            raster_size.0.to_bits().hash(&mut h);
            raster_size.1.to_bits().hash(&mut h);
            image.width.hash(&mut h);
            image.height.hash(&mut h);
            image.pixels().hash(&mut h);
        }
    }
    h.finish()
}

fn hash_command<H: Hasher>(cmd: &VectorCommand, h: &mut H) {
    match cmd {
        VectorCommand::Path { data, style } => {
            0u8.hash(h);
            for v in data.verbs() {
                hash_verb(v, h);
            }
            hash_path_style(style).hash(h);
        }
        VectorCommand::PushLayer {
            opacity,
            backdrop_blur,
        } => {
            1u8.hash(h);
            opacity.to_bits().hash(h);
            backdrop_blur.to_bits().hash(h);
        }
        VectorCommand::PopLayer => 2u8.hash(h),
    }
}

fn hash_verb<H: Hasher>(v: &PathVerb, h: &mut H) {
    match v {
        PathVerb::MoveTo(p) => {
            0u8.hash(h);
            hash_point(p, h);
        }
        PathVerb::LineTo(p) => {
            1u8.hash(h);
            hash_point(p, h);
        }
        PathVerb::QuadTo { ctrl, to } => {
            2u8.hash(h);
            hash_point(ctrl, h);
            hash_point(to, h);
        }
        PathVerb::CubicTo { ctrl1, ctrl2, to } => {
            3u8.hash(h);
            hash_point(ctrl1, h);
            hash_point(ctrl2, h);
            hash_point(to, h);
        }
        PathVerb::Close => 4u8.hash(h),
    }
}

fn hash_point<H: Hasher>(p: &Point, h: &mut H) {
    p.x.to_bits().hash(h);
    p.y.to_bits().hash(h);
}
