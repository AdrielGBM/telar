mod raster;
#[cfg(test)]
mod tests;
mod vector;

use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use rustc_hash::{FxHashMap, FxHasher};
use usvg::tiny_skia_path::Transform as SkiaTransform;

use crate::{Color, DrawCommand};

use vector::convert_group;

#[derive(Debug, Clone, thiserror::Error)]
#[error("failed to parse SVG: {0}")]
pub struct SvgError(String);

// Memo key: (width bits, height bits, tint as packed rgba bits or None). f32 goes through `to_bits` so it is Eq/Hash.
type MemoKey = (u32, u32, Option<[u32; 4]>);

/// A parsed SVG document that converts to the renderer's `DrawCommand`s.
///
/// The vector path is the common case; SVGs using features without a drawing primitive fall back to a single rasterized `DrawCommand::Image`.
pub struct SvgData {
    // Content-derived (not a counter) so rebuilding the same SVG keeps a stable id for downstream caches.
    id: u64,
    // Retained for the raster fallback path.
    tree: usvg::Tree,
    // Intrinsic size in px (`tree.size()`).
    size: (f32, f32),
    memo: Mutex<FxHashMap<MemoKey, Arc<Vec<DrawCommand>>>>,
}

// Marks conversion of a feature we have no primitive for; the caller then rasterizes the whole tree.
struct Unsupported;

impl SvgData {
    // Named `from_str` per the public API; it is fallible on content, not a `FromStr` impl.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(svg: &str) -> Result<Self, SvgError> {
        let opt = usvg::Options::default();
        let tree = usvg::Tree::from_str(svg, &opt).map_err(|e| SvgError(e.to_string()))?;
        let size = tree.size();
        let mut hasher = FxHasher::default();
        svg.as_bytes().hash(&mut hasher);
        Ok(Self {
            id: hasher.finish(),
            tree,
            size: (size.width(), size.height()),
            memo: Mutex::new(FxHashMap::default()),
        })
    }

    /// Stable, content-derived id. Same content parses to the same id across instances.
    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn intrinsic_size(&self) -> (f32, f32) {
        self.size
    }

    /// Display list in the widget's LOCAL space (`0,0..width,height`), memoized per `(width, height, tint)`.
    pub fn commands_for(
        &self,
        width: f32,
        height: f32,
        tint: Option<Color>,
    ) -> Arc<Vec<DrawCommand>> {
        let key: MemoKey = (
            width.to_bits(),
            height.to_bits(),
            tint.map(|c| [c.r.to_bits(), c.g.to_bits(), c.b.to_bits(), c.a.to_bits()]),
        );
        let mut memo = self.memo.lock().unwrap();
        if let Some(cached) = memo.get(&key) {
            return Arc::clone(cached);
        }
        let commands = Arc::new(self.build_commands(width, height, tint));
        // Simple bound: drop the whole cache rather than track an LRU; these display lists are cheap to rebuild.
        if memo.len() >= 16 {
            memo.clear();
        }
        memo.insert(key, Arc::clone(&commands));
        commands
    }

    fn build_commands(&self, width: f32, height: f32, tint: Option<Color>) -> Vec<DrawCommand> {
        let (vb_w, vb_h) = self.size;
        if vb_w <= 0.0 || vb_h <= 0.0 || width <= 0.0 || height <= 0.0 {
            return Vec::new();
        }
        // Uniform scale, centered (xMidYMid meet letterbox).
        let s = (width / vb_w).min(height / vb_h);
        let fitted_w = vb_w * s;
        let fitted_h = vb_h * s;
        let offset_x = (width - fitted_w) * 0.5;
        let offset_y = (height - fitted_h) * 0.5;
        let fit_ts = SkiaTransform::from_row(s, 0.0, 0.0, s, offset_x, offset_y);

        let mut out = Vec::new();
        match convert_group(self.tree.root(), fit_ts, tint, &mut out) {
            Ok(()) => out,
            Err(Unsupported) => self.raster_fallback(fitted_w, fitted_h, offset_x, offset_y, tint),
        }
    }
}
