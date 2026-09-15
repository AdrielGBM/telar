use geometry_core::Rect;
use renderer_core::BorderRadius;
use smallvec::{SmallVec, smallvec};
use tiny_skia::Mask;

use super::pixels::{PixelBounds, clamp_to_pixels, fill_mask_region};

#[derive(Clone, Copy, PartialEq)]
pub(super) struct ClipShape {
    pub(super) rect: Rect,
    pub(super) radius: BorderRadius,
}

struct Shown {
    shape: ClipShape,
    origin: (i32, i32),
    within: Option<SmallVec<[Rect; 8]>>,
}

pub(super) struct ClipMask {
    mask: Mask,
    painted: SmallVec<[PixelBounds; 8]>,
    shown: Option<Shown>,
}

impl ClipMask {
    pub(super) fn new(width: u32, height: u32) -> Option<Self> {
        Some(Self {
            mask: Mask::new(width, height)?,
            painted: SmallVec::new(),
            shown: None,
        })
    }

    pub(super) fn fits(&self, width: u32, height: u32) -> bool {
        self.mask.width() == width && self.mask.height() == height
    }

    // `origin` is where the mask's first pixel sits in window space; `within`, in the mask's own pixels, is the only area the shape may cover.
    pub(super) fn show(
        &mut self,
        shape: ClipShape,
        origin: (i32, i32),
        within: Option<&[Rect]>,
    ) -> &Mask {
        let current = self.shown.as_ref().is_some_and(|shown| {
            shown.shape == shape && shown.origin == origin && shown.within.as_deref() == within
        });
        if !current {
            self.paint(shape, origin, within);
            self.shown = Some(Shown {
                shape,
                origin,
                within: within.map(SmallVec::from_slice),
            });
        }
        &self.mask
    }

    fn paint(&mut self, shape: ClipShape, origin: (i32, i32), within: Option<&[Rect]>) {
        let (width, height) = (self.mask.width(), self.mask.height());
        let stride = width as usize;
        for region in self.painted.drain(..) {
            fill_mask_region(self.mask.data_mut(), stride, region, 0);
        }
        let local = Rect::new(
            shape.rect.x - origin.0 as f32,
            shape.rect.y - origin.1 as f32,
            shape.rect.width,
            shape.rect.height,
        );
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
        if shape.radius.is_zero() {
            for piece in &pieces {
                fill_mask_region(self.mask.data_mut(), stride, *piece, u8::MAX);
            }
            self.painted = pieces;
            return;
        }
        if pieces.is_empty() {
            return;
        }
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
