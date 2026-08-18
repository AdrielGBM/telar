use geometry_core::Transform;
use layout_core::{LayoutError, LayoutStyle};
use renderer_core::{BorderRadius, Color, RectStyle, ShapeStyle};
use theme_core::use_theme_tokens;
use ui_core::{LayoutItem, StyledContainer, box_item};

use crate::shared;
use crate::shared::props_default;

/// A determinate 0.0..=1.0 progress bar: a rounded track with an accent fill scaled by `value`. Sibling of
/// `slider` minus the drag/thumb — the fill reuses the same "absolute_fill child, scaled from the left edge"
/// technique (see `slider`'s fill) since a progress bar is a slider whose value the app drives instead of the
/// pointer. `value` is a closure so a reading derived from several services can drive it, and so an unbound
/// bar reads a flat zero rather than owning a signal nobody can write.
pub struct ProgressProps {
    /// Progress, normalized to 0.0..=1.0 (out-of-range inputs are clamped on read, not here).
    ///
    /// A closure rather than a signal, like [`color`](Self::color) beside it: a bar *reports* a reading, it
    /// never writes one, and a caller whose reading is derived from two services has no signal to hand over.
    /// Insisting on one is what makes a shell reimplement this widget next to the catalogue.
    pub value: Box<dyn Fn() -> f32>,
    /// Fill accent. `Color::TRANSPARENT` (the default) means "unset": fall back to the theme accent.
    pub color: Box<dyn Fn() -> Color>,
    /// Track (rail) colour; `Color::TRANSPARENT` (the default) means "unset": fall back to the theme's muted token.
    pub track_color: Box<dyn Fn() -> Color>,
    /// Track width in px. `0.0` (the default) means "unset" — the bar uses `220.0`. Ignored under
    /// [`stretch`](Self::stretch).
    pub width: f32,
    /// Fill the parent's width instead of taking a fixed one — what a bar inside a card wants, where a px
    /// track is either short of the card or past its edge.
    pub stretch: bool,
    /// Track height in px. `0.0` (the default) means "unset" — the bar uses `8.0`.
    pub height: f32,
}

props_default!(ProgressProps {
    value: reading,
    color: color,
    track_color: color,
    width: zero,
    stretch: zero,
    height: zero,
});

pub fn progress(props: ProgressProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let ProgressProps {
        value,
        color,
        track_color,
        width,
        stretch,
        height,
    } = props;
    // Shared by the fill's style closure and its transform, which both need the reading.
    let value: std::rc::Rc<dyn Fn() -> f32> = std::rc::Rc::from(value);
    let scale_value = std::rc::Rc::clone(&value);
    let width = if width > 0.0 { width } else { 220.0 };
    let height = if height > 0.0 { height } else { 8.0 };
    // Unlike `slider`, `color` is only needed by this one style closure, so it moves in directly — no `Rc`
    // re-erasure needed (that's only for props shared across several closures, e.g. fill + thumb).

    // The fill: an `absolute_fill` child (so it exactly overlays the track) scaled horizontally from the left
    // edge by `value` — cheaper than relaying out a narrower box on every update. See `slider`'s fill for the
    // same technique; `box_transform` only pivots scale on the rect centre, so the raw matrix is needed here
    // too to pin the left edge and grow rightward.

    let fill = StyledContainer::new(
        LayoutStyle::new().absolute_fill(),
        move |_r| {
            let fill = shared::resolve(color.as_ref(), || shared::accent());
            RectStyle::default()
                .with_fill(fill)
                .with_radius(BorderRadius::all(height / 2.0))
        },
        vec![],
    )?
    .with_transform(move |r| {
        let v = scale_value().clamp(0.0, 1.0);
        Some(Transform::scale_around(v, 1.0, r.x, r.y + r.height / 2.0).to_array())
    });

    let track = StyledContainer::new(
        // A stretched bar takes the parent's width: a px track inside a card is either short of it or
        // past its edge, and the card is what decides how wide a reading should read.
        LayoutStyle::new()
            .width(if stretch {
                layout_core::SizeDimension::Percent(1.0)
            } else {
                layout_core::SizeDimension::Px(width)
            })
            .height(height),
        move |_r| {
            let fill = shared::resolve(track_color.as_ref(), || {
                use_theme_tokens()
                    .map(|t| t.muted())
                    .unwrap_or(shared::DEFAULT_TRACK)
            });
            RectStyle::default()
                .with_fill(fill)
                .with_radius(BorderRadius::all(height / 2.0))
        },
        vec![box_item(fill)],
    )?;
    Ok(box_item(track))
}

#[cfg(test)]
mod tests {

    use reactive_core::signal;
    use ui_core::NodeId;

    use super::*;

    // Lays `node` out inside a 300×100 root, mirroring `slider`'s test harness.
    fn lay_out(node: NodeId) {
        crate::harness::lay_out(node, 300.0, 100.0);
    }

    // An unset `value` prop must fall back to a working internal signal (uncontrolled mode), not panic.
    #[test]
    fn uncontrolled_progress_builds_with_default_value() {
        crate::test_support::fresh_layout_runtime();
        let result = progress(ProgressProps::default());
        assert!(result.is_ok());
    }

    // A caller-bound signal must build, layout, and keep reporting the value it was given.
    #[test]
    fn controlled_progress_builds_and_layouts() {
        crate::test_support::fresh_layout_runtime();
        let value = signal(0.3f32);
        let widget = progress(ProgressProps {
            value: {
                let v = value.clone();
                Box::new(move || v.get())
            },
            width: 200.0,
            height: 10.0,
            ..Default::default()
        })
        .unwrap();
        lay_out(widget.layout_node());
        assert_eq!(value.get(), 0.3);
    }

    // Updating a bound value after the widget is built and laid out must not panic, including out-of-range
    // inputs (clamped where the fill's transform reads it, not on the signal itself).
    #[test]
    fn setting_value_after_build_does_not_panic() {
        crate::test_support::fresh_layout_runtime();
        let value = signal(0.0f32);
        let widget = progress(ProgressProps {
            value: {
                let v = value.clone();
                Box::new(move || v.get())
            },
            ..Default::default()
        })
        .unwrap();
        lay_out(widget.layout_node());
        value.set(0.75);
        value.set(1.5);
        assert_eq!(value.get(), 1.5);
    }

    /// The reason this prop is a closure. A reading derived from two services has no signal behind it, and a
    /// bar that insisted on one is a bar an application reimplements next to the catalogue.
    #[test]
    fn a_derived_reading_can_drive_the_bar() {
        reactive_core::reset_runtime();
        let used = signal(3.0f32);
        let total = signal(4.0f32);
        let fraction = reactive_core::derive_pair(used.clone(), total.clone(), |u, t| u / t);
        let read = fraction.clone();
        let bar = progress(ProgressProps {
            value: Box::new(move || read.get()),
            stretch: true,
            ..Default::default()
        });
        assert!(bar.is_ok(), "a derivation drives it");
        used.set(1.0);
        assert_eq!(fraction.get(), 0.25, "and keeps following its sources");
    }
}
