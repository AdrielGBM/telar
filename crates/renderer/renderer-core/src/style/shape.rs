use geometry_core::Rect;

use crate::{BorderRadius, Color};

use super::paint::{FillRule, Paint, Shadow, Stroke};

pub trait ShapeStyle: Sized {
    fn fill_mut(&mut self) -> &mut Option<Paint>;
    fn stroke_mut(&mut self) -> &mut Option<Stroke>;
    fn shadow_mut(&mut self) -> &mut Option<Shadow>;

    fn with_fill(mut self, fill: impl Into<Paint>) -> Self {
        *self.fill_mut() = Some(fill.into());
        self
    }
    fn with_stroke(mut self, stroke: Stroke) -> Self {
        *self.stroke_mut() = Some(stroke);
        self
    }
    fn with_shadow(mut self, shadow: Shadow) -> Self {
        *self.shadow_mut() = Some(shadow);
        self
    }
}

/// How thick a rect's border is on each side.
///
/// [`Uniform`](Self::Uniform) is everything a [`Stroke`] on its own can say, and all a path or a line ever
/// means by width: one number, applied the whole way round. [`PerSide`](Self::PerSide) is the case a box has
/// and a path does not — a rule under a header, a divider down one edge — where a side of `0.0` is simply not
/// there. Which is why the four numbers live on the rect rather than on the stroke they share a colour with.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum BorderWidths {
    #[default]
    Uniform,
    PerSide {
        top: f32,
        right: f32,
        bottom: f32,
        left: f32,
    },
}

impl BorderWidths {
    pub fn per_side(top: f32, right: f32, bottom: f32, left: f32) -> Self {
        Self::PerSide {
            top,
            right,
            bottom,
            left,
        }
    }

    /// The four thicknesses in `[top, right, bottom, left]` order, with [`Uniform`](Self::Uniform) taking its
    /// number from the stroke it belongs to.
    pub fn resolve(self, stroke_width: f32) -> [f32; 4] {
        match self {
            Self::Uniform => [stroke_width; 4],
            Self::PerSide {
                top,
                right,
                bottom,
                left,
            } => [top, right, bottom, left],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RectStyle {
    pub fill: Option<Paint>,
    pub stroke: Option<Stroke>,
    pub shadow: Option<Shadow>,
    pub radius: BorderRadius,
    /// Which sides of the border are drawn and how thick each is; see [`BorderWidths`]. Read it through
    /// [`border`](Self::border) rather than directly, so the uniform case resolves against `stroke.width`.
    pub border_widths: BorderWidths,
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

    /// Draws the stroke on the named sides only, at the named thicknesses. See [`BorderWidths`].
    pub fn with_border_widths(mut self, widths: BorderWidths) -> Self {
        self.border_widths = widths;
        self
    }

    /// The border this style actually paints: its paint, and the four thicknesses in
    /// `[top, right, bottom, left]` order. `None` when there is nothing to draw — no stroke at all, or every
    /// side sitting at zero.
    pub fn border(&self) -> Option<(Paint, [f32; 4])> {
        let stroke = self.stroke?;
        let widths = self.border_widths.resolve(stroke.width);
        widths
            .iter()
            .any(|w| *w > 0.0)
            .then_some((stroke.paint, widths))
    }
}

/// The inner edge of a border: the box pulled in by each side's thickness, with the corners tightened to
/// match.
///
/// Shared rather than derived twice, because the rasterizer and the GPU have to agree to the pixel on where a
/// border stops — one builds a path from it and the other an SDF, and a rule under a header that lands a half
/// pixel apart between backends is a bug nobody can see until they switch machines.
///
/// `None` when the border leaves no interior at all: the box is thinner than its own frame, and the frame
/// swallows it whole.
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
    // A corner is pulled in by the thicker of the two sides meeting there. CSS uses an ellipse when they
    // differ; `BorderRadius` holds one number per corner, and of the two the thicker side is the one that
    // would otherwise cut across its own arc.
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
    fn stroke_mut(&mut self) -> &mut Option<Stroke> {
        &mut self.stroke
    }
    fn shadow_mut(&mut self) -> &mut Option<Shadow> {
        &mut self.shadow
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
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
}

impl ShapeStyle for PathStyle {
    fn fill_mut(&mut self) -> &mut Option<Paint> {
        &mut self.fill
    }
    fn stroke_mut(&mut self) -> &mut Option<Stroke> {
        &mut self.stroke
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
    fn a_plain_stroke_still_means_all_four_sides() {
        let style = RectStyle::default().with_stroke(Stroke::new(Color::BLACK, 2.0));
        assert_eq!(style.border(), Some((Paint::Solid(Color::BLACK), [2.0; 4])));
    }

    /// The whole point of the type: a colour with no side to paint it on draws nothing, rather than falling
    /// back to the stroke's own width and framing the box.
    #[test]
    fn every_side_at_zero_is_no_border_at_all() {
        let style = RectStyle::default()
            .with_stroke(Stroke::new(Color::BLACK, 2.0))
            .with_border_widths(BorderWidths::per_side(0.0, 0.0, 0.0, 0.0));
        assert_eq!(style.border(), None);
    }

    #[test]
    fn a_side_of_its_own_overrides_the_strokes_width() {
        let style = RectStyle::default()
            .with_stroke(Stroke::new(Color::BLACK, 2.0))
            .with_border_widths(BorderWidths::per_side(0.0, 0.0, 1.0, 0.0));
        assert_eq!(
            style.border(),
            Some((Paint::Solid(Color::BLACK), [0.0, 0.0, 1.0, 0.0]))
        );
    }

    #[test]
    fn a_colourless_box_has_no_border_however_thick_its_sides_are() {
        let style =
            RectStyle::default().with_border_widths(BorderWidths::per_side(4.0, 4.0, 4.0, 4.0));
        assert_eq!(style.border(), None);
    }

    /// A side at zero leaves the inner edge flush with the outer one there, which is what makes the ring
    /// cover nothing along it — the rasterizer's two boundaries coincide and the shader's two SDFs agree.
    #[test]
    fn a_side_at_zero_leaves_the_inner_edge_flush_with_the_outer() {
        let (inner, _) =
            border_inner_shape(box_100(), BorderRadius::zero(), [0.0, 0.0, 1.0, 0.0]).unwrap();
        assert_eq!(inner, Rect::new(0.0, 0.0, 100.0, 99.0));
    }

    /// The uniform case has to come out exactly where the old single-stroke path put it: outer edge on the
    /// box, inner edge one width in, corners `r - w`.
    #[test]
    fn a_uniform_border_insets_every_side_and_tightens_every_corner() {
        let (inner, radius) =
            border_inner_shape(box_100(), BorderRadius::all(8.0), [2.0; 4]).unwrap();
        assert_eq!(inner, Rect::new(2.0, 2.0, 96.0, 96.0));
        assert_eq!(radius, BorderRadius::all(6.0));
    }

    /// Two sides of different thickness meet at a corner, and only one number is available to describe the
    /// arc between them.
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

    /// The border is thicker than the box it frames, so there is no interior left to punch out and the
    /// caller paints the box solid instead.
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
