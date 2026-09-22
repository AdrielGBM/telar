//! Choosing a value by how wide the surface is: a base value, and a step taking over from each width up.

use std::rc::Rc;

use reactive_core::{Memo, memo};

/// A value per width range of the surface: `base` below the first threshold, and each step's value from its width up to the next step's — CSS's `min-width` media queries, as a value instead of a stylesheet, so the same ranges choose a gutter on a page, a window and a terminal alike.
#[derive(Clone, Debug, PartialEq)]
pub struct Breakpoints<T> {
    base: T,
    steps: Vec<(f32, T)>,
}

/// Starts a set of [`Breakpoints`] from the value below every threshold.
pub fn breakpoint<T>(base: T) -> Breakpoints<T> {
    Breakpoints {
        base,
        steps: Vec::new(),
    }
}

impl<T> Breakpoints<T> {
    /// `value` from `min_width` up, until a wider step takes over. Steps may come in any order, and a second one at the same width replaces the first.
    pub fn at(mut self, min_width: f32, value: T) -> Self {
        match self.steps.iter().position(|(from, _)| *from >= min_width) {
            Some(i) if self.steps[i].0 == min_width => self.steps[i].1 = value,
            Some(i) => self.steps.insert(i, (min_width, value)),
            None => self.steps.push((min_width, value)),
        }
        self
    }

    /// Which range `width` falls in: `0` below every threshold, `n` from the n-th narrowest step up.
    pub fn range_at(&self, width: f32) -> usize {
        self.steps
            .iter()
            .take_while(|(from, _)| width >= *from)
            .count()
    }

    /// The value for a surface `width` wide.
    pub fn value_at(&self, width: f32) -> &T {
        self.value_of(self.range_at(width))
    }

    fn value_of(&self, range: usize) -> &T {
        match range.checked_sub(1) {
            Some(step) => &self.steps[step].1,
            None => &self.base,
        }
    }
}

impl<T: Clone + PartialEq + 'static> Breakpoints<T> {
    /// The value for the width of the surface this is called on, re-resolved only when that width crosses into another range: resizing within one range re-runs nothing that reads it. Read it where a style is built — `pad:$gutter` in a `[view]` — and the style follows it on the same terms.
    pub fn follow(self) -> Memo<T> {
        let breakpoints = Rc::new(self);
        let range = {
            let breakpoints = breakpoints.clone();
            memo(move || breakpoints.range_at(layout_reactive::use_surface_width()))
        };
        memo(move || breakpoints.value_of(range.get()).clone())
    }
}

#[cfg(test)]
#[path = "breakpoints_test.rs"]
mod tests;
