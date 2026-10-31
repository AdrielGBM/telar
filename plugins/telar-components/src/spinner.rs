//! [`spinner`]: an indeterminate ring, for work with no bound value to show.

use std::time::Duration;
use telar::{
    AlignItems, Border, Color, JustifyContent, LayoutError, LayoutItem, LayoutStyle, Props,
    Reactive, RectStyle, ShapeStyle, StyledContainer, SurfaceAmendment, amend_surface, box_item,
    box_transform, children,
    motion::{Easing, Keyframes, Repeat},
};

use crate::shared;

/// A spinner's own tokens, which an application's theme supplies through a `#[theme(extension)]` field.
///
/// Every field left `None` follows the role named beside it, read when the spinner paints.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SpinnerTokens {
    /// Diameter of the ring when the caller names no `size`, at the regular control size and scaled with the control size in force. `None` is [`metrics::control_height`](crate::metrics::control_height), so a spinner stands level in a row of controls.
    pub size: Option<f32>,
}

impl SpinnerTokens {
    /// The tokens of the theme in force, subscribing the caller to it.
    pub fn current() -> Self {
        telar::use_theme_extension::<Self>()
    }

    pub fn size(&self) -> f32 {
        self.size
            .map_or_else(shared::control_height, |size| size * telar::control_scale())
    }
}

/// Track stroke thickness and orbiting head diameter, as fractions of the ring's diameter, so a spinner a caller sizes keeps its proportions.
const TRACK_STROKE_FRACTION: f32 = 0.12;
const HEAD_FRACTION: f32 = 0.22;
/// One full rotation's duration — fast enough to read as "busy", slow enough not to strobe.
const ROTATION: Duration = Duration::from_millis(900);

/// An indeterminate spinner: a static ring (the track) with a small accent dot orbiting it forever. Unlike `progress`, there is no bound value to visualize, only "work is happening" — and a *uniformly*-stroked ring looks identical at every rotation angle, which would make spinning it pointless. A swept-colour stroke (transparent -> accent) was the first idea, but `Stroke`'s paint has no true angular/conic gradient, and a linear `Gradient` stroke on a rect silently degrades to its first stop's flat colour on the hardware renderer backend (`RectInstance` only carries `stroke_color: [f32; 4]`, taken via `Paint::solid_color()` — see `renderer-hardware/src/primitives/rect.rs`), so that would render as an invisible fully-transparent ring there. Orbiting a small solid-filled dot instead only needs primitives already proven correct on both renderer backends: a plain `RectStyle` fill plus `box_transform`'s rotation, which pivots on the box's own centre — rotating the dot's parent by an ever-advancing angle carries the dot around the ring with no per-frame trigonometry. Rotation is driven by `telar::motion::Keyframes` under `Repeat::Loop`: the same self-driving, indefinitely-repeating primitive `motion.rsx`'s equalizer bars use with `Repeat::PingPong`, here a single 0 -> 360 degree leg instead.
#[derive(Props)]
pub struct SpinnerProps {
    /// Head (orbiting dot) accent. `Color::TRANSPARENT` (the default) means "unset": fall back to the theme accent.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub color: Reactive<Color>,
    /// Ring diameter in px. `0.0` (the default) means "unset": the [`SpinnerTokens`] size at the control size in force.
    #[props(default)]
    pub size: f32,
    /// Amends the paint of the ring — the spinner's **principal surface**. See [`telar::SurfaceStyle`] for when a theme token or [`SpinnerTokens`] is the right instrument instead.
    #[props(some, default)]
    pub style: Option<SurfaceAmendment>,
}

/// An indeterminate ring, for work with no bound value to show.
pub fn spinner(props: SpinnerProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let SpinnerProps { color, size, style } = props;
    let size = move || {
        if size > 0.0 {
            size
        } else {
            SpinnerTokens::current().size()
        }
    };
    let head_box = move || {
        let head = size() * HEAD_FRACTION;
        LayoutStyle::new().width(head).height(head)
    };
    let ring_box = move || LayoutStyle::new().width(size()).height(size());

    let angle = Keyframes::<f32>::new(0.0)
        .then(360.0, ROTATION, Easing::Linear)
        .start(Repeat::Loop);

    let head = StyledContainer::new(
        head_box(),
        move |r| {
            let fill = shared::resolve(&color, shared::accent);
            RectStyle::default()
                .with_fill(fill)
                .with_radius(shared::pill(r))
        },
        vec![],
    )?
    .styled_by(head_box);

    // The rotor overlays the track and centres the head against its top edge, so rotating it on its own centre carries the head around the circle.
    let rotor = StyledContainer::new(
        LayoutStyle::column()
            .absolute_fill()
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::START),
        |_r| RectStyle::default(),
        children![head],
    )?
    .with_transform(move |r| box_transform(r, angle.get(), 1.0, 1.0, 0.0, 0.0));

    let track = StyledContainer::new(
        ring_box(),
        move |r| {
            let rest = RectStyle::default()
                .with_border(Border::uniform(
                    shared::muted(),
                    r.width.min(r.height) * TRACK_STROKE_FRACTION,
                ))
                .with_radius(shared::pill(r));
            amend_surface(rest, &style)
        },
        children![rotor],
    )?
    .styled_by(ring_box);

    Ok(box_item(track))
}

#[cfg(test)]
#[path = "spinner_test.rs"]
mod tests;
