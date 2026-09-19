//! The one modifier convention every value-stepping control shares: Shift is coarse, Alt is fine.

use platform_core::ModifiersState;

/// How much a step grows while Shift is held.
pub const COARSE_STEP: f32 = 10.0;
/// How much a step shrinks while Alt is held.
pub const FINE_STEP: f32 = 0.1;

/// The factor a step, a drag delta or an arrow press is multiplied by under `modifiers`. Both held cancel out.
pub fn step_factor(modifiers: ModifiersState) -> f32 {
    let coarse = if modifiers.is_shift { COARSE_STEP } else { 1.0 };
    let fine = if modifiers.is_alt { FINE_STEP } else { 1.0 };
    coarse * fine
}

#[cfg(test)]
#[path = "step_test.rs"]
mod tests;
