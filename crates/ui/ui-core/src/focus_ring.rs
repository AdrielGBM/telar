//! [`FocusRing`]: the ring that shows where the keyboard is, drawn the same way by every box that wears it.

use geometry_core::{Color, Rect};
use renderer_core::{Border, BorderRadius, Paint, RectStyle};
use theme_core::use_theme_tokens;
use ui_tree::RenderNode;

/// The ring a control wears while the keyboard is on it, and the cursor a list, a tree or a toolbar draws on the item the arrows reached: an outer band and, just inside it, a hairline half as wide in a second colour.
///
/// Two colours because no one colour stands out from everything a ring lands on. A band in the accent is invisible on a control filled with the accent, and a band in the surface colour is invisible on the surface around it; with both, the hairline separates the band from the fill and the band separates the hairline from the surface, so one of the two always shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FocusRing {
    /// The outer band, on the box's edge or [`focus_ring_offset`](theme_core::ThemeTokens::focus_ring_offset) inside it.
    pub band: Color,
    /// The hairline just inside the band, between it and the box's fill.
    pub hairline: Color,
    /// Width of the band; the hairline is half of it.
    pub width: f32,
}

/// The theme's ring: [`focus_ring`](theme_core::ThemeTokens::focus_ring) over [`focus_ring_inner`](theme_core::ThemeTokens::focus_ring_inner), [`focus_ring_width`](theme_core::ThemeTokens::focus_ring_width) wide.
pub fn focus_ring() -> FocusRing {
    let tokens = use_theme_tokens();
    FocusRing {
        band: tokens.focus_ring(),
        hairline: tokens.focus_ring_inner(),
        width: tokens.focus_ring_width(),
    }
}

impl FocusRing {
    /// The same ring with its outer band in `band`: a list drawing its cursor in a colour of its own keeps the theme's hairline.
    pub fn with_band(self, band: Color) -> Self {
        Self { band, ..self }
    }

    /// Width of the hairline inside the band.
    pub fn hairline_width(&self) -> f32 {
        self.width / 2.0
    }

    /// Each band as the frame it is drawn with and how far inside the ring's outer edge it starts, outermost first.
    ///
    /// A surface drawn in whole cells has one colour per cell behind one glyph, so it cannot put a hairline beside a band: it draws one band, in whichever colour stands out more from the fill under it.
    fn bands(self, fill: Option<&Paint>) -> Vec<(Border, f32)> {
        if geometry_core::layout_grid().is_unit() {
            return vec![
                (Border::uniform(self.band, self.width), 0.0),
                (
                    Border::uniform(self.hairline, self.hairline_width()),
                    self.width,
                ),
            ];
        }
        let colour = match fill {
            Some(Paint::Solid(fill)) if fill.a >= 1.0 => {
                if self.hairline.contrast_ratio(*fill) > self.band.contrast_ratio(*fill) {
                    self.hairline
                } else {
                    self.band
                }
            }
            _ => self.band,
        };
        vec![(Border::uniform(colour, self.width), 0.0)]
    }

    /// What the ring adds on a surface drawn in whole cells under a box that has no fill of its own.
    ///
    /// A terminal draws the ring as box-drawing characters in the cells around the content, and a box its content fills, such as a link with no padding, has no such cells: the text is drawn over its own frame. A cell keeps its background under a glyph, so a tint is what still shows there.
    pub(crate) fn cell_tint(self) -> Option<Paint> {
        if geometry_core::layout_grid().is_unit() {
            return None;
        }
        Some(self.band.with_alpha(0.35).into())
    }

    /// The ring over a box of `rect` and `radius`, `offset` inside its edge: the band that lands on the edge itself, which takes the place of the box's own border, and a rect for every band standing inside it.
    pub(crate) fn place(
        self,
        rect: Rect,
        radius: BorderRadius,
        offset: f32,
        fill: Option<&Paint>,
    ) -> (Option<Border>, Vec<RenderNode>) {
        let mut edge = None;
        let mut inside = Vec::new();
        for (border, inset) in self.bands(fill) {
            let inset = offset + inset;
            if inset > 0.0 {
                inside.extend(band_inside(rect, radius, border, inset));
            } else {
                edge = Some(border);
            }
        }
        (edge, inside)
    }

    /// The ring as rects of its own, for a widget that draws it over its content rather than as its own frame.
    pub(crate) fn nodes(self, rect: Rect, radius: BorderRadius, offset: f32) -> Vec<RenderNode> {
        let (edge, inside) = self.place(rect, radius, offset, None);
        edge.map(|border| {
            RenderNode::rect(
                rect,
                RectStyle::default().with_border(border).with_radius(radius),
            )
        })
        .into_iter()
        .chain(inside)
        .collect()
    }
}

/// `border` drawn `inset` inside the box, on a shape of its own that follows the box's corners. `None` where the box is too small to hold it.
pub(crate) fn band_inside(
    rect: Rect,
    radius: BorderRadius,
    border: Border,
    inset: f32,
) -> Option<RenderNode> {
    let (inner, radius) = renderer_core::border_inner_shape(rect, radius, [inset; 4])?;
    Some(RenderNode::rect(
        inner,
        RectStyle::default().with_border(border).with_radius(radius),
    ))
}

#[cfg(test)]
#[path = "focus_ring.test.rs"]
mod tests;
