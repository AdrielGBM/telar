use geometry_core::Transform;
use layout_core::{LayoutError, LayoutStyle};
use reactive_core::{RwSignal, signal};
use renderer_core::{BorderRadius, Color, RectStyle, ShapeStyle};
use theme_core::use_theme_tokens;
use ui_core::{LayoutItem, StyledContainer, box_item};

use crate::shared;
use crate::shared::props_default;

/// A determinate 0.0..=1.0 progress bar: a rounded track with an accent fill scaled by `value`. Sibling of
/// `slider` minus the drag/thumb — the fill reuses the same "absolute_fill child, scaled from the left edge"
/// technique (see `slider`'s fill) since a progress bar is a slider whose value the app drives instead of the
/// pointer. `value` is `Option` so `Props` can derive `Default`: `None` is uncontrolled (the widget owns its
/// own signal), `Some` is caller-bound.
pub struct ProgressProps {
    /// Bound progress, normalized to 0.0..=1.0 (out-of-range inputs are clamped on read, not here).
    /// `None` (the default) is uncontrolled — the widget makes its own `signal(0.0)`.
    pub value: Option<RwSignal<f32>>,
    /// Fill accent. `Color::TRANSPARENT` (the default) means "unset": fall back to the theme accent.
    pub color: Box<dyn Fn() -> Color>,
    /// Track (rail) colour; `Color::TRANSPARENT` (the default) means "unset": fall back to the theme's muted token.
    pub track_color: Box<dyn Fn() -> Color>,
    /// Track width in px. `0.0` (the default) means "unset" — the bar uses `220.0`.
    pub width: f32,
    /// Track height in px. `0.0` (the default) means "unset" — the bar uses `8.0`.
    pub height: f32,
}

props_default!(ProgressProps {
    value: none,
    color: color,
    track_color: color,
    width: zero,
    height: zero,
});

pub fn progress(props: ProgressProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let ProgressProps {
        value,
        color,
        track_color,
        width,
        height,
    } = props;
    // Uncontrolled: own the value so the bar still works when the caller binds no signal.
    let value = value.unwrap_or_else(|| signal(0.0));
    let width = if width > 0.0 { width } else { 220.0 };
    let height = if height > 0.0 { height } else { 8.0 };
    // Unlike `slider`, `color` is only needed by this one style closure, so it moves in directly — no `Rc`
    // re-erasure needed (that's only for props shared across several closures, e.g. fill + thumb).

    // The fill: an `absolute_fill` child (so it exactly overlays the track) scaled horizontally from the left
    // edge by `value` — cheaper than relaying out a narrower box on every update. See `slider`'s fill for the
    // same technique; `box_transform` only pivots scale on the rect centre, so the raw matrix is needed here
    // too to pin the left edge and grow rightward.
    let fill_value = value.clone();
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
        let v = fill_value.get().clamp(0.0, 1.0);
        Some(Transform::scale_around(v, 1.0, r.x, r.y + r.height / 2.0).to_array())
    });

    let track = StyledContainer::new(
        LayoutStyle::new().width(width).height(height),
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
            value: Some(value.clone()),
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
            value: Some(value.clone()),
            ..Default::default()
        })
        .unwrap();
        lay_out(widget.layout_node());
        value.set(0.75);
        value.set(1.5);
        assert_eq!(value.get(), 1.5);
    }
}
