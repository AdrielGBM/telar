//! Timing functions, and the cubic-Bézier solver behind the CSS-named ones.

/// Timing function mapping normalized time `t` in `[0, 1]` to eased progress.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Easing {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    /// CSS `cubic-bezier(x1, y1, x2, y2)` with control points P1/P2 (P0=(0,0), P3=(1,1)).
    CubicBezier(f32, f32, f32, f32),
    /// CSS `steps(n, <jumpterm>)`: `n` flat plateaus instead of a continuous curve.
    Steps(u32, StepPosition),
}

/// Where a [`Easing::Steps`] plateau jumps relative to its interval, matching CSS `<step-position>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StepPosition {
    /// Jumps at the start of each interval (CSS `jump-start` / legacy `start`).
    JumpStart,
    /// Jumps at the end of each interval (CSS `jump-end` / legacy `end`); the CSS default.
    #[default]
    JumpEnd,
    /// No jump at either boundary: `t=0` and `t=1` hold flat (CSS `jump-none`, needs `n >= 2`).
    JumpNone,
    /// Jumps at both boundaries, adding one extra plateau (CSS `jump-both`).
    JumpBoth,
}

// Newton-Raphson iteration cap before falling back to bisection.
const NEWTON_ITERS: u32 = 8;
// Below this |dX/du| Newton steps are unreliable, so switch to bisection.
const NEWTON_MIN_SLOPE: f32 = 1e-6;
const BISECTION_ITERS: u32 = 32;
const SUBDIVISION_EPS: f32 = 1e-7;

impl Easing {
    pub fn apply(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Easing::Linear => t,
            Easing::EaseIn => t * t * t,
            Easing::EaseOut => {
                let u = 1.0 - t;
                1.0 - u * u * u
            }
            Easing::EaseInOut => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    let u = -2.0 * t + 2.0;
                    1.0 - u * u * u / 2.0
                }
            }
            Easing::CubicBezier(x1, y1, x2, y2) => {
                let u = solve_bezier_x(t, x1, x2);
                bezier_axis(u, y1, y2)
            }
            Easing::Steps(n, position) => step_progress(n, position, t),
        }
    }
}

// CSS Easing Level 1 `step_easing_function`, specialized to `x` already clamped to `[0, 1]` by `apply`, which drops the spec's extrapolation branches.
fn step_progress(n: u32, position: StepPosition, x: f32) -> f32 {
    let n = n.max(1) as f32;
    let mut current = (x * n).floor();
    if matches!(position, StepPosition::JumpStart | StepPosition::JumpBoth) {
        current += 1.0;
    }
    if current < 0.0 {
        current = 0.0;
    }
    let jumps = match position {
        StepPosition::JumpStart | StepPosition::JumpEnd => n,
        StepPosition::JumpNone => (n - 1.0).max(1.0),
        StepPosition::JumpBoth => n + 1.0,
    };
    if current > jumps {
        current = jumps;
    }
    current / jumps
}

// One Bezier axis given its two control-point coordinates (endpoints are 0 and 1).
fn bezier_axis(u: f32, p1: f32, p2: f32) -> f32 {
    let c = 3.0 * p1;
    let b = 3.0 * (p2 - p1) - c;
    let a = 1.0 - c - b;
    ((a * u + b) * u + c) * u
}

fn bezier_axis_slope(u: f32, p1: f32, p2: f32) -> f32 {
    let c = 3.0 * p1;
    let b = 3.0 * (p2 - p1) - c;
    let a = 1.0 - c - b;
    (3.0 * a * u + 2.0 * b) * u + c
}

// Find the Bezier parameter u such that X(u) == x, Newton-Raphson with a bisection fallback.
fn solve_bezier_x(x: f32, x1: f32, x2: f32) -> f32 {
    let mut u = x;
    for _ in 0..NEWTON_ITERS {
        let error = bezier_axis(u, x1, x2) - x;
        if error.abs() < SUBDIVISION_EPS {
            return u;
        }
        let slope = bezier_axis_slope(u, x1, x2);
        if slope.abs() < NEWTON_MIN_SLOPE {
            break;
        }
        u -= error / slope;
    }
    let (mut low, mut high, mut u) = (0.0f32, 1.0f32, x.clamp(0.0, 1.0));
    for _ in 0..BISECTION_ITERS {
        let value = bezier_axis(u, x1, x2);
        if (value - x).abs() < SUBDIVISION_EPS {
            return u;
        }
        if value < x {
            low = u;
        } else {
            high = u;
        }
        u = (low + high) * 0.5;
    }
    u
}

#[cfg(test)]
#[path = "easing_test.rs"]
mod tests;
