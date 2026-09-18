use geometry_core::Rect;
use renderer_core::BorderRadius;
use renderer_core::perf::{self, Phase};
use smallvec::{SmallVec, smallvec};
use tiny_skia::Mask;

use super::pixels::{PixelBounds, clamp_to_pixels, fill_mask_region, multiply_mask_region};

#[derive(Clone, Copy, PartialEq)]
pub(super) struct ClipShape {
    pub(super) rect: Rect,
    pub(super) radius: BorderRadius,
}

// The clips one draw is nested in, which stays a handful however deep the tree gets.
type Ancestors = SmallVec<[ClipShape; 4]>;

struct Shown {
    shape: ClipShape,
    ancestors: Ancestors,
    origin: (i32, i32),
    within: Option<SmallVec<[Rect; 8]>>,
}

pub(super) struct ClipMask {
    mask: Mask,
    // A rounded ancestor's own coverage, which the mask is then multiplied by. Intersecting two shapes is a product of two rasterisations, and tiny-skia rasterises into a whole mask or not at all, so the second one needs a buffer of its own. Allocated only for the surfaces that nest one rounded clip inside another.
    outer: Option<Mask>,
    painted: SmallVec<[PixelBounds; 8]>,
    shown: Option<Shown>,
}

impl ClipMask {
    pub(super) fn new(width: u32, height: u32) -> Option<Self> {
        let _span = perf::span(Phase::Mask);
        Some(Self {
            mask: Mask::new(width, height)?,
            outer: None,
            painted: SmallVec::new(),
            shown: None,
        })
    }

    pub(super) fn fits(&self, width: u32, height: u32) -> bool {
        self.mask.width() == width && self.mask.height() == height
    }

    // `origin` is where the mask's first pixel sits in window space; `within`, in the mask's own pixels, is the only area the shape may cover. `ancestors` are the clips `shape` is nested in, outermost first: their rects are already folded into its own, but their rounded corners have to cut it too.
    pub(super) fn show(
        &mut self,
        shape: ClipShape,
        ancestors: &[ClipShape],
        origin: (i32, i32),
        within: Option<&[Rect]>,
    ) -> &Mask {
        let current = self.shown.as_ref().is_some_and(|shown| {
            shown.shape == shape
                && shown.ancestors.as_slice() == ancestors
                && shown.origin == origin
                && shown.within.as_deref() == within
        });
        if !current {
            self.paint(shape, ancestors, origin, within);
            self.shown = Some(Shown {
                shape,
                ancestors: Ancestors::from_slice(ancestors),
                origin,
                within: within.map(SmallVec::from_slice),
            });
        }
        &self.mask
    }

    fn paint(
        &mut self,
        shape: ClipShape,
        ancestors: &[ClipShape],
        origin: (i32, i32),
        within: Option<&[Rect]>,
    ) {
        let _span = perf::span(Phase::Mask);
        let (width, height) = (self.mask.width(), self.mask.height());
        let stride = width as usize;
        for region in self.painted.drain(..) {
            fill_mask_region(self.mask.data_mut(), stride, region, 0);
        }
        let local = local_rect(shape.rect, origin);
        let Some(bounds) = clamp_to_pixels(local, width, height) else {
            return;
        };
        let pieces: SmallVec<[PixelBounds; 8]> = match within {
            None => smallvec![bounds],
            Some(regions) => regions
                .iter()
                .filter_map(|region| clamp_to_pixels(*region, width, height))
                .filter_map(|region| overlap(bounds, region))
                .collect(),
        };
        if pieces.is_empty() {
            return;
        }
        if shape.radius.is_zero() {
            for piece in &pieces {
                fill_mask_region(self.mask.data_mut(), stride, *piece, u8::MAX);
            }
            self.painted = pieces;
        } else {
            if let Some(path) = crate::primitives::rect::build_rect_path(local, shape.radius) {
                self.mask.fill_path(
                    &path,
                    tiny_skia::FillRule::Winding,
                    true,
                    tiny_skia::Transform::identity(),
                );
            }
            if within.is_some() {
                zero_outside(self.mask.data_mut(), stride, bounds, &pieces);
            }
            self.painted.push(bounds);
        }
        for ancestor in ancestors.iter().filter(|a| !a.radius.is_zero()) {
            self.cut(*ancestor, origin, bounds);
        }
    }

    /// Multiplies the mask by a rounded ancestor's coverage where its corners fall, so a descendant drawn into one is cut by it.
    ///
    /// Its corners and nothing else: a clip is pushed already intersected with the ones around it, so every pixel `bounds` covers lies inside the ancestor's rect, and the quarter-disc at each corner is the only part of that rect the ancestor does not cover whole.
    fn cut(&mut self, shape: ClipShape, origin: (i32, i32), bounds: PixelBounds) {
        let (width, height) = (self.mask.width(), self.mask.height());
        let local = local_rect(shape.rect, origin);
        let corners: SmallVec<[PixelBounds; 4]> = corner_boxes(local, shape.radius)
            .into_iter()
            .filter_map(|corner| clamp_to_pixels(corner, width, height))
            .filter_map(|corner| overlap(bounds, corner))
            .collect();
        if corners.is_empty() {
            return;
        }
        let Some(path) = crate::primitives::rect::build_rect_path(local, shape.radius) else {
            return;
        };
        if self.outer.is_none() {
            self.outer = Mask::new(width, height);
        }
        let Some(outer) = &mut self.outer else {
            return;
        };
        let stride = width as usize;
        // `fill_path` draws over what is already in the buffer, so the corners about to be read back start from zero. What an earlier cut left elsewhere stays there, unread.
        for corner in &corners {
            fill_mask_region(outer.data_mut(), stride, *corner, 0);
        }
        outer.fill_path(
            &path,
            tiny_skia::FillRule::Winding,
            true,
            tiny_skia::Transform::identity(),
        );
        for corner in &corners {
            multiply_mask_region(self.mask.data_mut(), outer.data(), stride, *corner);
        }
    }
}

// A shape in the mask's own pixels, `origin` being where the mask's first pixel sits in window space.
fn local_rect(rect: Rect, origin: (i32, i32)) -> Rect {
    Rect::new(
        rect.x - origin.0 as f32,
        rect.y - origin.1 as f32,
        rect.width,
        rect.height,
    )
}

// The square each corner's arc curves inside, clamped the way the path builder clamps its radii.
fn corner_boxes(rect: Rect, radius: BorderRadius) -> [Rect; 4] {
    let (w, h) = (rect.width, rect.height);
    let side = |r: f32| r.min(w / 2.0).min(h / 2.0).max(0.0);
    let (tl, tr, br, bl) = (
        side(radius.top_left),
        side(radius.top_right),
        side(radius.bottom_right),
        side(radius.bottom_left),
    );
    [
        Rect::new(rect.x, rect.y, tl, tl),
        Rect::new(rect.x + w - tr, rect.y, tr, tr),
        Rect::new(rect.x + w - br, rect.y + h - br, br, br),
        Rect::new(rect.x, rect.y + h - bl, bl, bl),
    ]
}

fn overlap(a: PixelBounds, b: PixelBounds) -> Option<PixelBounds> {
    let (x0, y0) = (a.0.max(b.0), a.1.max(b.1));
    let (x1, y1) = (a.2.min(b.2), a.3.min(b.3));
    (x1 > x0 && y1 > y0).then_some((x0, y0, x1, y1))
}

fn zero_outside(data: &mut [u8], stride: usize, bounds: PixelBounds, pieces: &[PixelBounds]) {
    let (x0, y0, x1, y1) = bounds;
    for y in y0..y1 {
        let mut spans: SmallVec<[(u32, u32); 8]> = pieces
            .iter()
            .filter(|piece| piece.1 <= y && y < piece.3)
            .map(|piece| (piece.0, piece.2))
            .collect();
        spans.sort_unstable();
        let row = &mut data[y as usize * stride..(y as usize + 1) * stride];
        let mut x = x0;
        for (start, end) in spans {
            if start > x {
                row[x as usize..start as usize].fill(0);
            }
            x = x.max(end);
        }
        if x < x1 {
            row[x as usize..x1 as usize].fill(0);
        }
    }
}
