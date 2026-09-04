//! The paint a box or path is drawn with: fill, border, corner radius and shadow.

use geometry_core::Rect;

use crate::{BorderRadius, Color};

use super::paint::{FillRule, Paint, Shadow, Stroke};

/// What a fillable shape shares. The outline is *not* here: a path is stroked, with a cap and a join that mean something along an open curve, and a box is framed, with a thickness per side that a path has no use for. They were one method while a rect carried a `Stroke` it only ever read two fields of.
pub trait ShapeStyle: Sized {
    fn fill_mut(&mut self) -> &mut Option<Paint>;
    fn shadow_mut(&mut self) -> &mut Option<Shadow>;

    fn with_fill(mut self, fill: impl Into<Paint>) -> Self {
        *self.fill_mut() = Some(fill.into());
        self
    }
    fn with_shadow(mut self, shadow: Shadow) -> Self {
        *self.shadow_mut() = Some(shadow);
        self
    }
}

/// A box's frame: what it is painted with, and how thick it is on each side.
///
/// One value where a rect used to carry a [`Stroke`] beside a separate `BorderWidths` whose uniform case resolved against that stroke's `width` — so the thickness was defined in two places at once, and two of the stroke's four fields were dead: no rect path in either renderer has ever read `cap` or `join`. A `Stroke` still means all four things for a [`PathStyle`] and a line, where they all matter.
///
/// Four numbers rather than one because a side of `0.0` is simply not there: a rule under a header, a divider down one edge. `Border::uniform` is the common case spelled once.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Border {
    pub paint: Paint,
    /// Thicknesses in `[top, right, bottom, left]` order.
    pub widths: [f32; 4],
}

impl Border {
    /// The same thickness the whole way round.
    pub fn uniform(paint: impl Into<Paint>, width: f32) -> Self {
        Self {
            paint: paint.into(),
            widths: [width; 4],
        }
    }

    pub fn per_side(paint: impl Into<Paint>, top: f32, right: f32, bottom: f32, left: f32) -> Self {
        Self {
            paint: paint.into(),
            widths: [top, right, bottom, left],
        }
    }

    /// Whether this frame puts anything on screen at all — every side at zero draws nothing.
    pub fn is_visible(&self) -> bool {
        self.widths.iter().any(|w| *w > 0.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
/// The paint a box is drawn with: fill, border, corner radius, shadow and opacity.
pub struct RectStyle {
    pub fill: Option<Paint>,
    /// The frame around the box. See [`Border`].
    pub border: Option<Border>,
    pub shadow: Option<Shadow>,
    pub radius: BorderRadius,
}

impl RectStyle {
    pub fn filled(color: Color, radius: f32) -> Self {
        Self {
            fill: Some(Paint::Solid(color)),
            radius: BorderRadius::all(radius),
            ..Self::default()
        }
    }

    pub fn with_radius(mut self, radius: BorderRadius) -> Self {
        self.radius = radius;
        self
    }

    pub fn with_border(mut self, border: Border) -> Self {
        self.border = Some(border);
        self
    }

    /// The border this style actually paints: its paint, and the four thicknesses in `[top, right, bottom, left]` order. `None` when there is nothing to draw.
    pub fn painted_border(&self) -> Option<(Paint, [f32; 4])> {
        let border = self.border?;
        border.is_visible().then_some((border.paint, border.widths))
    }
}

/// The inner edge of a border: the box pulled in by each side's thickness, with the corners tightened to match.
///
/// Shared rather than derived twice, because the rasterizer and the GPU have to agree to the pixel on where a border stops — one builds a path from it and the other an SDF, and a rule under a header that lands a half pixel apart between backends is a bug nobody can see until they switch machines.
///
/// `None` when the border leaves no interior at all: the box is thinner than its own frame, and the frame swallows it whole.
pub fn border_inner_shape(
    rect: Rect,
    radius: BorderRadius,
    widths: [f32; 4],
) -> Option<(Rect, BorderRadius)> {
    let [top, right, bottom, left] = widths;
    let width = rect.width - left - right;
    let height = rect.height - top - bottom;
    if !(width > 0.0 && height > 0.0) {
        return None;
    }
    let max_r = width.min(height) * 0.5;
    // A corner is pulled in by the thicker of the two sides meeting there. CSS uses an ellipse when they differ; `BorderRadius` holds one number per corner, and of the two the thicker side is the one that would otherwise cut across its own arc.
    let tighten = |r: f32, a: f32, b: f32| (r - a.max(b)).clamp(0.0, max_r);
    let inner_radius = BorderRadius {
        top_left: tighten(radius.top_left, top, left),
        top_right: tighten(radius.top_right, top, right),
        bottom_right: tighten(radius.bottom_right, bottom, right),
        bottom_left: tighten(radius.bottom_left, bottom, left),
    };
    Some((
        Rect::new(rect.x + left, rect.y + top, width, height),
        inner_radius,
    ))
}

impl ShapeStyle for RectStyle {
    fn fill_mut(&mut self) -> &mut Option<Paint> {
        &mut self.fill
    }
    fn shadow_mut(&mut self) -> &mut Option<Shadow> {
        &mut self.shadow
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
/// The paint a path is drawn with: fill, stroke, fill rule and shadow.
pub struct PathStyle {
    pub fill: Option<Paint>,
    pub stroke: Option<Stroke>,
    pub shadow: Option<Shadow>,
    pub fill_rule: FillRule,
}

impl PathStyle {
    pub fn with_fill_rule(mut self, rule: FillRule) -> Self {
        self.fill_rule = rule;
        self
    }

    /// Strokes the path. A cap and a join mean something along an open curve, which is why this is the path's own and not a box's frame.
    pub fn with_stroke(mut self, stroke: Stroke) -> Self {
        self.stroke = Some(stroke);
        self
    }
}

impl ShapeStyle for PathStyle {
    fn fill_mut(&mut self) -> &mut Option<Paint> {
        &mut self.fill
    }
    fn shadow_mut(&mut self) -> &mut Option<Shadow> {
        &mut self.shadow
    }
}

#[cfg(test)]
mod border_tests {
    use super::*;

    fn box_100() -> Rect {
        Rect::new(0.0, 0.0, 100.0, 100.0)
    }

    #[test]
    fn a_uniform_border_means_all_four_sides() {
        let style = RectStyle::default().with_border(Border::uniform(Color::BLACK, 2.0));
        assert_eq!(
            style.painted_border(),
            Some((Paint::Solid(Color::BLACK), [2.0; 4]))
        );
    }

    /// The whole point of the type: a colour with no side to paint it on draws nothing.
    #[test]
    fn every_side_at_zero_is_no_border_at_all() {
        let style =
            RectStyle::default().with_border(Border::per_side(Color::BLACK, 0.0, 0.0, 0.0, 0.0));
        assert_eq!(style.painted_border(), None);
    }

    #[test]
    fn one_named_side_is_the_whole_frame() {
        let style =
            RectStyle::default().with_border(Border::per_side(Color::BLACK, 0.0, 0.0, 1.0, 0.0));
        assert_eq!(
            style.painted_border(),
            Some((Paint::Solid(Color::BLACK), [0.0, 0.0, 1.0, 0.0]))
        );
    }

    /// Thicknesses with no paint to draw them in used to be representable — a `border_widths` beside a `stroke: None`. Now there is nothing to construct.
    #[test]
    fn a_box_with_no_border_paints_none() {
        assert_eq!(RectStyle::default().painted_border(), None);
    }

    /// A side at zero leaves the inner edge flush with the outer one there, which is what makes the ring cover nothing along it — the rasterizer's two boundaries coincide and the shader's two SDFs agree.
    #[test]
    fn a_side_at_zero_leaves_the_inner_edge_flush_with_the_outer() {
        let (inner, _) =
            border_inner_shape(box_100(), BorderRadius::zero(), [0.0, 0.0, 1.0, 0.0]).unwrap();
        assert_eq!(inner, Rect::new(0.0, 0.0, 100.0, 99.0));
    }

    /// The uniform case has to come out exactly where the old single-stroke path put it: outer edge on the box, inner edge one width in, corners `r - w`.
    #[test]
    fn a_uniform_border_insets_every_side_and_tightens_every_corner() {
        let (inner, radius) =
            border_inner_shape(box_100(), BorderRadius::all(8.0), [2.0; 4]).unwrap();
        assert_eq!(inner, Rect::new(2.0, 2.0, 96.0, 96.0));
        assert_eq!(radius, BorderRadius::all(6.0));
    }

    /// Two sides of different thickness meet at a corner, and only one number is available to describe the arc between them.
    #[test]
    fn a_corner_is_tightened_by_the_thicker_of_the_two_sides_that_meet_there() {
        let (_, radius) =
            border_inner_shape(box_100(), BorderRadius::all(10.0), [1.0, 0.0, 0.0, 4.0]).unwrap();
        assert_eq!(radius.top_left, 6.0, "top 1, left 4 — the left side wins");
        assert_eq!(radius.top_right, 9.0, "top 1, right 0");
        assert_eq!(radius.bottom_right, 10.0, "neither side is drawn");
        assert_eq!(radius.bottom_left, 6.0, "bottom 0, left 4");
    }

    #[test]
    fn a_corner_never_tightens_past_straight() {
        let (_, radius) = border_inner_shape(box_100(), BorderRadius::all(2.0), [8.0; 4]).unwrap();
        assert_eq!(radius, BorderRadius::zero());
    }

    /// The border is thicker than the box it frames, so there is no interior left to punch out and the caller paints the box solid instead.
    #[test]
    fn a_border_thicker_than_its_box_leaves_no_interior() {
        assert!(
            border_inner_shape(box_100(), BorderRadius::zero(), [60.0, 0.0, 60.0, 0.0]).is_none()
        );
        assert!(
            border_inner_shape(box_100(), BorderRadius::zero(), [0.0, 50.0, 0.0, 50.0]).is_none()
        );
    }
}
