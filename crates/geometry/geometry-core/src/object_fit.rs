//! How sized content is scaled into the box it was given, mirroring CSS `object-fit`.

use crate::Rect;

/// How a sized piece of content (an image or SVG) is scaled into its layout box, mirroring CSS `object-fit`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ObjectFit {
    /// Stretch to fill the box exactly, ignoring the intrinsic aspect ratio (may distort).
    Fill,
    /// Scale uniformly to fit inside the box, centered, leaving letterbox gaps (no clipping).
    #[default]
    Contain,
    /// Scale uniformly to cover the box, centered; the overflow must be clipped to the box.
    Cover,
    /// Like [`Contain`](Self::Contain) but by a whole number, centered.
    ///
    /// The variant CSS has no name for, and the only one a pixel-art image can use: at a fractional scale some source pixels land on four screen pixels and their neighbours on five, so the grid the artist drew stops being a grid. Flooring the scale keeps every pixel the same size and spends the remainder on a wider letterbox.
    ///
    /// Falls back to `Contain` when the content does not fit even once, since there is no whole number below one to floor to.
    ContainInteger,
}

/// Places `intrinsic`-sized content into `container` per `fit`.
///
/// Returns the content rect (in `container`'s coordinate space) and whether the caller must clip it to `container` — true only for `Cover`, whose content deliberately overflows.
pub fn fit_rect(intrinsic: (f32, f32), container: Rect, fit: ObjectFit) -> (Rect, bool) {
    let (iw, ih) = intrinsic;
    // A zero-area intrinsic or container has no defined aspect ratio to preserve; fall back to filling the box.
    if iw <= 0.0 || ih <= 0.0 || container.width <= 0.0 || container.height <= 0.0 {
        return (container, false);
    }
    match fit {
        ObjectFit::Fill => (container, false),
        ObjectFit::Contain | ObjectFit::Cover | ObjectFit::ContainInteger => {
            let sx = container.width / iw;
            let sy = container.height / ih;
            let (s, clip) = match fit {
                ObjectFit::Cover => (sx.max(sy), true),
                ObjectFit::ContainInteger => (sx.min(sy).floor().max(1.0).min(sx.min(sy)), false),
                _ => (sx.min(sy), false),
            };
            let w = iw * s;
            let h = ih * s;
            let x = container.x + (container.width - w) * 0.5;
            let y = container.y + (container.height - h) * 0.5;
            (Rect::new(x, y, w, h), clip)
        }
    }
}

#[cfg(test)]
#[path = "object_fit_test.rs"]
mod tests;
