[logic]
use std::time::Duration;

use crate::shared;
use telar::ControlSize;
use telar::motion::{Easing, Keyframes, Repeat};

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

/// An indeterminate ring, for work with no bound value to show: a static track with a small accent dot orbiting it forever.
///
/// A reader hears it as a progress bar with no reading, which says the work is under way and not how far along it is, called what the call's `a11y_label` names it.
pub struct Props {
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

fn ring(own: Option<f32>, size: ControlSize) -> f32 {
    own.unwrap_or_else(|| SpinnerTokens::current().size(size))
}

let size = props.size;
let own = props.diameter;
let color = props.color.clone();
let angle = Keyframes::<f32>::new(0.0)
    .then(360.0, ROTATION, Easing::Linear)
    .start(Repeat::Loop);

[view]
box width:(ring(own, size)) height:(ring(own, size)) stroke:$theme.muted stroke_width:(ring(own, size) * TRACK_STROKE_FRACTION) radius:full role:progressbar surface:$props.style
    // A uniformly stroked ring looks the same at every angle and a stroke cannot carry a swept colour, so the motion is a solid dot at the top edge of a box that fills the track and turns on its own centre.
    col absolute:fill align:center justify:start rotate:(angle.get())
        box width:(ring(own, size) * HEAD_FRACTION) height:(ring(own, size) * HEAD_FRACTION) fill:(shared::resolve(&color, shared::accent)) radius:full
