//! [`spinner`]: an indeterminate ring, for work with no bound value to show.

use std::time::Duration;
use telar::{
    AlignItems, Border, Color, JustifyContent, LayoutError, LayoutItem, LayoutStyle, Props,
    Reactive, RectStyle, Role, ShapeStyle, StyledContainer, SurfaceAmendment, amend_surface,
    box_item, box_transform, children,
    motion::{Easing, Keyframes, Repeat},
};

use crate::shared;
use telar::ControlSize;

/// A spinner's own tokens, which an application's theme supplies through a `#[theme(extension)]` field.
///
/// Every field left `None` follows the role named beside it, read when the spinner paints.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SpinnerTokens {
    /// Diameter of the ring at the regular size, scaled with the spinner's own. `None` is [`metrics::control_height`](crate::metrics::control_height) at that size, so a spinner stands level in a row of controls of its size.
    pub size: Option<f32>,
}

impl SpinnerTokens {
    /// The tokens of the theme in force, subscribing the caller to it.
    pub fn current() -> Self {
        telar::use_theme_extension::<Self>()
    }

    /// The diameter of the ring of a spinner of `size`.
    pub fn size(&self, size: ControlSize) -> f32 {
        self.size.map_or_else(
            || shared::control_height(size),
            |diameter| diameter * size.scale(),
        )
    }
}

/// Track stroke thickness and orbiting head diameter, as fractions of the ring's diameter, so a spinner a caller sizes keeps its proportions.
const TRACK_STROKE_FRACTION: f32 = 0.12;
const HEAD_FRACTION: f32 = 0.22;
/// One full rotation's duration — fast enough to read as "busy", slow enough not to strobe.
const ROTATION: Duration = Duration::from_millis(900);

/// An indeterminate spinner: a static ring (the track) with a small accent dot orbiting it forever. A uniformly-stroked ring looks identical at every rotation angle, and a swept-colour stroke has no conic gradient to draw with (a linear `Gradient` stroke on a rect degrades to its first stop's flat colour on the hardware renderer, since `RectInstance` carries only `stroke_color`), so the motion is a solid dot whose parent rotates about its own centre through `box_transform`, driven by `telar::motion::Keyframes` under `Repeat::Loop`, with no per-frame trigonometry.
#[derive(Props)]
pub struct SpinnerProps {
    /// Head (orbiting dot) accent. `None` (the default) is the theme accent.
    #[props(some, into, default)]
    pub color: Option<Reactive<Color>>,
    /// How large the ring is drawn, as [`ControlSize`] works it out from [`SpinnerTokens::size`].
    #[props(default)]
    pub size: ControlSize,
    /// The ring's diameter in px. `None` (the default) is what [`size`](Self::size) works out.
    #[props(some, default)]
    pub diameter: Option<f32>,
    /// Amends the paint of the ring — the spinner's **principal surface**. See [`telar::SurfaceStyle`] for when a theme token or [`SpinnerTokens`] is the right instrument instead.
    #[props(some, default)]
    pub style: Option<SurfaceAmendment>,
}

/// An indeterminate ring, for work with no bound value to show.
///
/// A reader hears it as a progress bar with no reading, which says the work is under way and not how far along it is, called what the call's `a11y_label` names it.
pub fn spinner(props: SpinnerProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let SpinnerProps {
        color,
        size,
        diameter,
        style,
    } = props;
    let size = move || {
        diameter.unwrap_or_else(|| SpinnerTokens::current().size(size))
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
    .styled_by(ring_box)
    .role(Role::ProgressBar);

    Ok(box_item(track))
}

#[cfg(test)]
#[path = "spinner.test.rs"]
mod tests;
