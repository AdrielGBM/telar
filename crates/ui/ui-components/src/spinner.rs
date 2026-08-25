use std::time::Duration;

use layout_core::{AlignItems, JustifyContent, LayoutError, LayoutStyle};
use motion_core::{Easing, Keyframes, Repeat};
use renderer_core::{Border, BorderRadius, Color, RectStyle, ShapeStyle};
use ui_core::{LayoutItem, StyledContainer, box_item, box_transform};

use crate::shared;
use crate::shared::props_default;

/// Track stroke thickness and orbiting head diameter, as fractions of `size`.
const TRACK_STROKE_FRACTION: f32 = 0.12;
const HEAD_FRACTION: f32 = 0.22;
/// One full rotation's duration — fast enough to read as "busy", slow enough not to strobe.
const ROTATION: Duration = Duration::from_millis(900);

/// An indeterminate spinner: a static ring (the track) with a small accent dot orbiting it forever. Unlike
/// `progress`, there is no bound value to visualize, only "work is happening" — and a *uniformly*-stroked
/// ring looks identical at every rotation angle, which would make spinning it pointless. A swept-colour stroke
/// (transparent -> accent) was the first idea, but `Stroke`'s paint has no true angular/conic gradient, and a
/// linear `Gradient` stroke on a rect silently degrades to its first stop's flat colour on the hardware
/// renderer backend (`RectInstance` only carries `stroke_color: [f32; 4]`, taken via `Paint::solid_color()` —
/// see `renderer-hardware/src/primitives/rect.rs`), so that would render as an invisible fully-transparent
/// ring there. Orbiting a small solid-filled dot instead only needs primitives already proven correct on both
/// renderer backends: a plain `RectStyle` fill plus `box_transform`'s rotation, which pivots on the box's own
/// centre — rotating the dot's parent by an ever-advancing angle carries the dot around the ring with no
/// per-frame trigonometry. Rotation is driven by `motion_core::Keyframes` under `Repeat::Loop`: the same
/// self-driving, indefinitely-repeating primitive `motion.rsx`'s equalizer bars use with `Repeat::PingPong`,
/// here a single 0 -> 360 degree leg instead.
pub struct SpinnerProps {
    /// Head (orbiting dot) accent. `Color::TRANSPARENT` (the default) means "unset": fall back to the theme accent.
    pub color: Box<dyn Fn() -> Color>,
    /// Ring diameter in px. `0.0` (the default) means "unset" — the spinner uses `24.0`.
    pub size: f32,
}

props_default!(SpinnerProps {
    color: color,
    size: zero,
});

pub fn spinner(props: SpinnerProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let SpinnerProps { color, size } = props;
    let size = if size > 0.0 { size } else { 24.0 };
    let head_size = size * HEAD_FRACTION;

    // Continuous 0 -> 360 loop; `box_transform` turns each frame's angle into a rotation pivoted on the
    // rotor's own centre, which lands on the ring's centre (the rotor is sized to exactly overlay it below).
    let angle = Keyframes::<f32>::new(0.0)
        .then(360.0, ROTATION, Easing::Linear)
        .start(Repeat::Loop);

    // `color` is only needed by this one style closure, so it moves in directly — no `Rc` re-erasure needed
    // (that's only for props shared across several closures, e.g. `slider`'s fill + thumb).
    let head = StyledContainer::new(
        LayoutStyle::new().width(head_size).height(head_size),
        move |_r| {
            let fill = shared::resolve(color.as_ref(), || shared::accent());
            RectStyle::default()
                .with_fill(fill)
                .with_radius(BorderRadius::all(head_size / 2.0))
        },
        vec![],
    )?;

    // The rotor exactly overlays the track (`absolute_fill`) and centres the head against its top edge;
    // rotating the whole rotor carries the head around the circle as `angle` advances.
    let rotor = StyledContainer::new(
        LayoutStyle::new()
            .absolute_fill()
            .flex_column()
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::START),
        |_r| RectStyle::default(),
        vec![box_item(head)],
    )?
    .with_transform(move |r| box_transform(r, angle.get(), 1.0, 1.0, 0.0, 0.0));

    let track = StyledContainer::new(
        LayoutStyle::new().width(size).height(size),
        move |_r| {
            RectStyle::default()
                .with_border(Border::uniform(
                    shared::muted(),
                    size * TRACK_STROKE_FRACTION,
                ))
                .with_radius(BorderRadius::all(size / 2.0))
        },
        vec![box_item(rotor)],
    )?;

    Ok(box_item(track))
}

#[cfg(test)]
mod tests {

    use ui_core::NodeId;

    use super::*;

    // Lays `node` out inside a 100×100 root, mirroring `slider`'s test harness.
    fn lay_out(node: NodeId) {
        crate::harness::lay_out(node, 100.0, 100.0);
    }

    #[test]
    fn spinner_builds_with_default_size() {
        crate::test_support::fresh_layout_runtime();
        let widget = spinner(SpinnerProps::default());
        assert!(widget.is_ok());
        lay_out(widget.unwrap().layout_node());
    }

    #[test]
    fn spinner_builds_with_custom_size() {
        crate::test_support::fresh_layout_runtime();
        let widget = spinner(SpinnerProps {
            size: 48.0,
            ..Default::default()
        })
        .unwrap();
        lay_out(widget.layout_node());
    }
}
