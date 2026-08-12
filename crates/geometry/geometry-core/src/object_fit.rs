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
    /// The variant CSS has no name for, and the only one a pixel-art image can use: at a fractional scale
    /// some source pixels land on four screen pixels and their neighbours on five, so the grid the artist
    /// drew stops being a grid. Flooring the scale keeps every pixel the same size and spends the remainder
    /// on a wider letterbox.
    ///
    /// Falls back to `Contain` when the content does not fit even once, since there is no whole number
    /// below one to floor to.
    ContainInteger,
}

/// Places `intrinsic`-sized content into `container` per `fit`.
///
/// Returns the content rect (in `container`'s coordinate space) and whether the caller must clip
/// it to `container` — true only for `Cover`, whose content deliberately overflows.
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
mod tests {
    use super::*;

    #[test]
    fn fill_returns_container_and_no_clip() {
        let c = Rect::new(0.0, 0.0, 120.0, 60.0);
        let (rect, clip) = fit_rect((10.0, 10.0), c, ObjectFit::Fill);
        assert_eq!(rect, c);
        assert!(!clip);
    }

    #[test]
    fn contain_letterboxes_wide_box() {
        // 10x10 into 120x60: uniform scale = min(12, 6) = 6, fitted 60x60, centered horizontally.
        let c = Rect::new(0.0, 0.0, 120.0, 60.0);
        let (rect, clip) = fit_rect((10.0, 10.0), c, ObjectFit::Contain);
        assert_eq!(rect, Rect::new(30.0, 0.0, 60.0, 60.0));
        assert!(!clip);
    }

    #[test]
    fn cover_overflows_and_clips() {
        // 10x10 into 120x60: uniform scale = max(12, 6) = 12, fitted 120x120, centered (overflows top/bottom).
        let c = Rect::new(0.0, 0.0, 120.0, 60.0);
        let (rect, clip) = fit_rect((10.0, 10.0), c, ObjectFit::Cover);
        assert_eq!(rect, Rect::new(0.0, -30.0, 120.0, 120.0));
        assert!(clip);
    }

    #[test]
    fn contain_respects_container_origin() {
        let c = Rect::new(5.0, 7.0, 120.0, 60.0);
        let (rect, _) = fit_rect((10.0, 10.0), c, ObjectFit::Contain);
        assert_eq!(rect, Rect::new(35.0, 7.0, 60.0, 60.0));
    }

    #[test]
    fn degenerate_intrinsic_fills() {
        let c = Rect::new(0.0, 0.0, 120.0, 60.0);
        let (rect, clip) = fit_rect((0.0, 10.0), c, ObjectFit::Contain);
        assert_eq!(rect, c);
        assert!(!clip);
    }

    #[test]
    fn default_is_contain() {
        assert_eq!(ObjectFit::default(), ObjectFit::Contain);
    }

    /// The case `Contain` gets wrong for pixel art: at a fractional scale the source grid stops being a
    /// grid, because some pixels round to one screen pixel more than their neighbours.
    #[test]
    fn contain_integer_floors_the_scale_and_widens_the_letterbox() {
        // 320x180 into 1300x740: Contain would take min(4.06, 4.11) and smear the grid; flooring to 4 gives 1280x720 centred, spending the remainder on the border instead.
        let c = Rect::new(0.0, 0.0, 1300.0, 740.0);
        let (rect, clip) = fit_rect((320.0, 180.0), c, ObjectFit::ContainInteger);
        assert_eq!(rect, Rect::new(10.0, 10.0, 1280.0, 720.0));
        assert!(!clip);
    }

    /// An exact multiple has nothing to floor, so it must not lose a step to rounding.
    #[test]
    fn contain_integer_fills_an_exact_multiple() {
        let c = Rect::new(0.0, 0.0, 1280.0, 720.0);
        let (rect, _) = fit_rect((320.0, 180.0), c, ObjectFit::ContainInteger);
        assert_eq!(rect, Rect::new(0.0, 0.0, 1280.0, 720.0));
    }

    /// Below one there is no whole number to floor to, so it behaves as `Contain` rather than vanishing.
    #[test]
    fn contain_integer_below_one_falls_back_to_fitting() {
        let c = Rect::new(0.0, 0.0, 160.0, 90.0);
        let (rect, _) = fit_rect((320.0, 180.0), c, ObjectFit::ContainInteger);
        assert_eq!(rect, Rect::new(0.0, 0.0, 160.0, 90.0));
    }
}
