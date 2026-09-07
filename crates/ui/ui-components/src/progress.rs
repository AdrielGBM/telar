//! [`progress`]: a determinate bar filled to a bound value.

use geometry_core::Transform;
use layout_core::{LayoutError, LayoutStyle};
use reactive_core::Reactive;
use renderer_core::{BorderRadius, Color, RectStyle, ShapeStyle};
use telar_macros::Props;
use ui_core::{Children, LayoutItem, StyledContainer, box_item};

use crate::shared;

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
    /// Track width in px. `0.0` (the default) means "unset" — the bar uses `220.0`. Ignored under [`stretch`](Self::stretch).
    #[props(default)]
    pub width: f32,
    /// Fill the parent's width instead of taking a fixed one — what a bar inside a card wants, where a px track is either short of the card or past its edge.
    #[props(default)]
    pub stretch: bool,
    /// Track height in px. `0.0` (the default) means "unset" — the bar uses `8.0`.
    #[props(default)]
    pub height: f32,
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
    } = props;
    // Shared by the fill's style closure and its transform, which both need the reading.
    let scale_value = value.clone();
    let width = if width > 0.0 { width } else { 220.0 };
    let height = if height > 0.0 { height } else { 8.0 };
    // Needed by this one style closure, so it moves in directly; `Rc` re-erasure is only for props shared across several closures.

    // An `absolute_fill` child, so it exactly overlays the track, scaled horizontally from the left edge — cheaper than relaying out a narrower box on every update. `box_transform` only pivots scale on the rect centre, so the raw matrix is needed to pin the left edge.

    let fill = StyledContainer::new(
        LayoutStyle::new().absolute_fill(),
        move |_r| {
            let fill = shared::resolve(&color, shared::accent);
            RectStyle::default()
                .with_fill(fill)
                .with_radius(BorderRadius::all(height / 2.0))
        },
        vec![],
    )?
    .with_transform(move |r| {
        let v = scale_value.get().clamp(0.0, 1.0);
        Some(Transform::scale_around(v, 1.0, r.x, r.y + r.height / 2.0).to_array())
    });

    let track = StyledContainer::new(
        // A stretched bar takes the parent's width: a px track inside a card is either short of its edge or past it, and the card is what decides how wide a reading should read.
        LayoutStyle::new()
            .width(if stretch {
                layout_core::SizeDimension::Percent(1.0)
            } else {
                layout_core::SizeDimension::Px(width)
            })
            .height(height),
        move |_r| {
            let fill = shared::resolve(&track_color, shared::muted);
            RectStyle::default()
                .with_fill(fill)
                .with_radius(BorderRadius::all(height / 2.0))
        },
        vec![box_item(fill)],
    )?;
    Ok(box_item(track))
}

#[cfg(test)]
#[path = "progress_test.rs"]
mod tests;
