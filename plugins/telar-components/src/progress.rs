//! [`progress`]: a determinate bar filled to a bound value.

use telar::{
    BorderRadius, Children, Color, LayoutError, LayoutItem, LayoutStyle, Props, Reactive, Rect,
    RectStyle, ShapeStyle, StyledContainer, SurfaceAmendment, Transform, amend_surface, box_item,
};

use crate::shared;

/// A progress bar's own tokens, which an application's theme supplies through a `#[theme(extension)]` field.
///
/// Every field left `None` follows the theme role or measure named beside it, read when the bar paints and lays out.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProgressTokens {
    /// Corner radius of the track and its fill. `None` rounds both ends fully, whatever the bar's thickness.
    pub radius: Option<f32>,
    /// The track's width when the bar names none, at the regular control size, scaled with the control size in force. `None` is 16 ems of the document's text.
    pub width: Option<f32>,
    /// The track's thickness when the bar names none, at the regular control size, scaled with the control size in force. `None` is [`metrics::gap_md`](crate::metrics::gap_md).
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

    pub fn width(&self) -> f32 {
        shared::width_or_measure(self.width, 16.0)
    }

    pub fn height(&self) -> f32 {
        self.height
            .map_or_else(shared::gap_md, |height| height * telar::control_scale())
    }
}

/// A determinate 0.0..=1.0 progress bar: a rounded track with an accent fill scaled by `value`. Sibling of `slider` minus the drag/thumb — the fill reuses the same "absolute_fill child, scaled from the left edge" technique (see `slider`'s fill) since a progress bar is a slider whose value the app drives instead of the pointer. `value` is a closure so a reading derived from several services can drive it, and so an unbound bar reads a flat zero rather than owning a signal nobody can write.
#[derive(Props)]
pub struct ProgressProps {
    /// Progress, normalized to 0.0..=1.0 (out-of-range inputs are clamped on read, not here).
    ///
    /// A closure rather than a signal, like [`color`](Self::color) beside it: a bar *reports* a reading, it never writes one, and a caller whose reading is derived from two services has no signal to hand over. Insisting on one is what makes a shell reimplement this widget next to the catalogue.
    #[props(into, default)]
    pub value: Reactive<f32>,
    /// Fill accent. `Color::TRANSPARENT` (the default) means "unset": fall back to the theme accent.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub color: Reactive<Color>,
    /// Track (rail) colour; `Color::TRANSPARENT` (the default) means "unset": fall back to the theme's muted token.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub track_color: Reactive<Color>,
    /// Track width in px. `0.0` (the default) means "unset" -> [`ProgressTokens::width`]. Ignored under [`stretch`](Self::stretch).
    #[props(default)]
    pub width: f32,
    /// Fill the parent's width instead of taking a fixed one — what a bar inside a card wants, where a px track is either short of the card or past its edge.
    #[props(default)]
    pub stretch: bool,
    /// Track height in px. `0.0` (the default) means "unset" -> [`ProgressTokens::height`].
    #[props(default)]
    pub height: f32,
    /// Amends the paint of the track — its **principal surface**. See [`telar::SurfaceStyle`] for when a theme token or [`ProgressTokens`] is the right instrument instead.
    #[props(some, default)]
    pub style: Option<SurfaceAmendment>,
}

/// A determinate bar filled to a bound value.
pub fn progress(
    props: ProgressProps,
    _children: Children,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let ProgressProps {
        value,
        color,
        track_color,
        width,
        stretch,
        height,
        style,
    } = props;
    let width = (width > 0.0).then_some(width);
    let height = (height > 0.0).then_some(height);

    // An `absolute_fill` child, so it exactly overlays the track, scaled horizontally from the left edge — cheaper than relaying out a narrower box on every update. `box_transform` only pivots scale on the rect centre, so the raw matrix is needed to pin the left edge.
    let fill = StyledContainer::new(
        LayoutStyle::new().absolute_fill(),
        move |r| {
            RectStyle::default()
                .with_fill(shared::resolve(&color, shared::accent))
                .with_radius(ProgressTokens::current().radius(r))
        },
        vec![],
    )?
    .with_transform(move |r| {
        let v = value.get().clamp(0.0, 1.0);
        Some(Transform::scale_around(v, 1.0, r.x, r.y + r.height / 2.0).to_array())
    });

    // A stretched bar takes the parent's width: a px track inside a card is either short of its edge or past it, and the card is what decides how wide a reading should read.
    let track_box = move || {
        let tokens = ProgressTokens::current();
        LayoutStyle::new()
            .width(if stretch {
                telar::SizeDimension::Percent(1.0)
            } else {
                telar::SizeDimension::Px(width.unwrap_or_else(|| tokens.width()))
            })
            .height(height.unwrap_or_else(|| tokens.height()))
    };
    let track = StyledContainer::new(
        track_box(),
        move |r| {
            let track = RectStyle::default()
                .with_fill(shared::resolve(&track_color, shared::muted))
                .with_radius(ProgressTokens::current().radius(r));
            amend_surface(track, &style)
        },
        vec![box_item(fill)],
    )?
    .styled_by(track_box);
    Ok(box_item(track))
}

#[cfg(test)]
#[path = "progress_test.rs"]
mod tests;
