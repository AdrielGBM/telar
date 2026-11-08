//! The one rule for turning a surface's physical pixels into logical ones.

/// The logical length of `physical` device pixels at `scale_factor`, rounded to the nearest whole pixel.
pub fn logical_extent(physical: u32, scale_factor: f64) -> u32 {
    (physical as f64 / scale_factor).round() as u32
}

#[cfg(test)]
#[path = "logical_extent.test.rs"]
mod tests;
