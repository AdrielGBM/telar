//! The two ways a value can travel: a [`Tween`] over a fixed duration, or a [`Spring`] under its own physics.

use std::time::Duration;

use crate::easing::Easing;

/// A time-based interpolation over a fixed `duration` shaped by `easing`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tween {
    pub duration: Duration,
    pub easing: Easing,
}

/// A physical spring parameterized by raw `stiffness`, `damping`, and `mass`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spring {
    pub stiffness: f32,
    pub damping: f32,
    pub mass: f32,
}

/// The motion model backing an [`crate::Animated`] value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Curve {
    Tween(Tween),
    Spring(Spring),
}

/// Build a [`Tween`] from a duration and easing.
pub fn tween(duration: Duration, easing: Easing) -> Tween {
    Tween { duration, easing }
}

/// Build a [`Spring`] from stiffness and damping; mass defaults to 1.0.
pub fn spring(stiffness: f32, damping: f32) -> Spring {
    Spring {
        stiffness,
        damping,
        mass: 1.0,
    }
}

impl Spring {
    /// Soft settle with essentially no overshoot; scale mirrors the near-critical spring(170, 26) used for the sandbox theme-color transition.
    pub fn gentle() -> Spring {
        spring(120.0, 14.0)
    }

    /// Fast, firm settle for interactive feedback (button presses, toggles).
    pub fn snappy() -> Spring {
        spring(210.0, 20.0)
    }

    /// Visible overshoot before settling; scale mirrors the underdamped spring(170, 12) used for the sandbox scale demo.
    pub fn bouncy() -> Spring {
        spring(180.0, 12.0)
    }
}

impl From<Tween> for Curve {
    fn from(t: Tween) -> Self {
        Curve::Tween(t)
    }
}

impl From<Spring> for Curve {
    fn from(s: Spring) -> Self {
        Curve::Spring(s)
    }
}
