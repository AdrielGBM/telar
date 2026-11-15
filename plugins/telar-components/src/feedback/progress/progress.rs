//! [`progress`]: a determinate bar filled to a bound value.

use telar::{
    BorderRadius, Color, Direction, LayoutError, LayoutItem, LayoutStyle, NumericValue, Props,
    Reactive, Rect, RectStyle, Role, ShapeStyle, StyledContainer, SurfaceAmendment, Transform,
    amend_surface, box_item, children,
};

use crate::shared;
use telar::ControlSize;

/// A progress bar's own tokens, which an application's theme supplies through a `#[theme(extension)]` field.
///
/// Every field left `None` follows the theme role or measure named beside it, read when the bar paints and lays out.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProgressTokens {
    /// Corner radius of the track and its fill. `None` rounds both ends fully, whatever the bar's thickness.
    pub radius: Option<f32>,
    /// The track's width when the bar names none, at the regular size, scaled with the bar's own. `None` is 16 ems of the document's text.
    pub width: Option<f32>,
    /// The track's thickness when the bar names none, at the regular size, scaled with the bar's own. `None` is [`metrics::gap_md`](crate::metrics::gap_md).
    pub height: Option<f32>,
}

impl ProgressTokens {
    /// The tokens of the theme in force, subscribing the caller to it.
    pub fn current() -> Self {
        telar::use_theme_extension::<Self>()
    }

    /// The corners of a bar laid out at `rect`.
    pub fn radius(&self, rect: Rect) -> BorderRadius {
        self.radius
            .map_or_else(|| shared::pill(rect), BorderRadius::all)
    }

    /// The track's width in a bar of `size` whose call names none.
    pub fn width(&self, size: ControlSize) -> f32 {
        shared::width_or_measure(size, self.width, 16.0)
    }

    /// The track's thickness in a bar of `size` whose call names none.
    pub fn height(&self, size: ControlSize) -> f32 {
        self.height
            .map_or_else(|| shared::gap_md(size), |height| height * size.scale())
    }
}

/// A determinate 0.0..=1.0 progress bar: a rounded track with an accent fill scaled by `value`. Sibling of `slider` minus the drag/thumb — the fill reuses the same "absolute_fill child, scaled from one edge" technique (see `slider`'s fill) since a progress bar is a slider whose value the app drives instead of the pointer, and it fills from the inline start: the left edge on a left-to-right surface, the right edge on a right-to-left one. `value` is a closure so a reading derived from several services can drive it, and so an unbound bar reads a flat zero rather than owning a signal nobody can write.
#[derive(Props)]
pub struct ProgressProps {
    /// Progress, normalized to 0.0..=1.0 (out-of-range inputs are clamped on read, not here).
    ///
    /// A closure rather than a signal, like [`color`](Self::color) beside it: a bar *reports* a reading, it never writes one, and a caller whose reading is derived from two services has no signal to hand over. Insisting on one is what makes a shell reimplement this widget next to the catalogue.
    #[props(into, default)]
    pub value: Reactive<f32>,
    /// How large the bar is drawn: its thickness and, unless it names one or stretches, its width, as [`ControlSize`] works them out.
    #[props(default)]
    pub size: ControlSize,
    /// Fill accent. `None` (the default) is the theme accent.
    #[props(some, into, default)]
    pub color: Option<Reactive<Color>>,
    /// Track (rail) colour; `None` (the default) is the theme's muted token.
    #[props(some, into, default)]
    pub track_color: Option<Reactive<Color>>,
    /// Track width in px. `None` (the default) is [`ProgressTokens::width`]. Ignored under [`stretch`](Self::stretch).
    #[props(some, default)]
    pub width: Option<f32>,
    /// Fill the parent's width instead of taking a fixed one — what a bar inside a card wants, where a px track is either short of the card or past its edge.
    #[props(default)]
    pub stretch: bool,
    /// Track height in px. `None` (the default) is [`ProgressTokens::height`].
    #[props(some, default)]
    pub height: Option<f32>,
    /// Amends the paint of the track — its **principal surface**. See [`telar::SurfaceStyle`] for when a theme token or [`ProgressTokens`] is the right instrument instead.
    #[props(some, default)]
    pub style: Option<SurfaceAmendment>,
}

/// A determinate bar filled to a bound value.
///
/// A reader hears it as a progress bar standing at its reading out of 100, called what the call's `a11y_label` names it.
pub fn progress(props: ProgressProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let ProgressProps {
        value,
        size,
        color,
        track_color,
        width,
        stretch,
        height,
        style,
    } = props;

    let filled = move || value.get().clamp(0.0, 1.0);

    // An `absolute_fill` child, so it exactly overlays the track, scaled horizontally from the edge the reading starts at — cheaper than relaying out a narrower box on every update. `box_transform` only pivots scale on the rect centre, so the raw matrix is needed to pin that edge.
    let fill = StyledContainer::new(
        LayoutStyle::new().absolute_fill(),
        move |r| {
            RectStyle::default()
                .with_fill(shared::resolve(&color, shared::accent))
                .with_radius(ProgressTokens::current().radius(r))
        },
        vec![],
    )?
    .with_transform({
        let filled = filled.clone();
        move |r| {
            let start = match telar::use_direction() {
                Direction::Ltr => r.x,
                Direction::Rtl => r.x + r.width,
            };
            Some(Transform::scale_around(filled(), 1.0, start, r.y + r.height / 2.0).to_array())
        }
    });

    // A stretched bar takes the parent's width: a px track inside a card is either short of its edge or past it, and the card is what decides how wide a reading should read.
    let track_box = move || {
        let tokens = ProgressTokens::current();
        LayoutStyle::new()
            .width(if stretch {
                telar::SizeDimension::Percent(1.0)
            } else {
                telar::SizeDimension::Px(width.unwrap_or_else(|| tokens.width(size)))
            })
            .height(height.unwrap_or_else(|| tokens.height(size)))
    };
    let track = StyledContainer::new(
        track_box(),
        move |r| {
            let track = RectStyle::default()
                .with_fill(shared::resolve(&track_color, shared::muted))
                .with_radius(ProgressTokens::current().radius(r));
            amend_surface(track, &style)
        },
        children![fill],
    )?
    .styled_by(track_box)
    .role(Role::ProgressBar)
    .valued(move || NumericValue {
        now: f64::from(filled()) * 100.0,
        min: 0.0,
        max: 100.0,
    });
    Ok(box_item(track))
}

#[cfg(test)]
#[path = "progress.test.rs"]
mod tests;
