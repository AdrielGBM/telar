//! The rsx motion engine: signal-backed values that chase a target over time,
//! driven by a central frame ticker. An [`Animated<T>`] wraps a reactive signal
//! whose value is interpolated toward `target` by a [`Curve`] (a [`Tween`] or a
//! [`Spring`]); [`tick`] advances every registered animation once per frame.
//! Colors interpolate in Oklch (see [`Lerp`]) as a deliberate perceptual choice.

mod animated;
mod curve;
mod easing;
mod keyframes;
mod lerp;
mod ticker;

pub use animated::Animated;
pub use curve::{Curve, Spring, Tween, spring, tween};
pub use easing::Easing;
pub use keyframes::{Keyframes, KeyframesBuilder, Repeat};
pub use lerp::Lerp;
pub use ticker::{has_active, reset, set_scale, tick};
