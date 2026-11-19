//! [`Bands`]: rects filed by the horizontal bands of a page they cross, so the few that reach a line can be found without looking at all of them.

use std::collections::HashMap;

use crate::Rect;

/// How tall one band is, in logical px.
const BAND: f32 = 64.0;

/// The most bands one rect is filed under; a taller one is checked on every query instead.
const MOST_BANDS: i64 = 512;

/// A list of rects, each named by its place in it, filed by the 64 px bands of the page each one crosses from its top edge to its bottom edge, both included.
///
/// What a check over a long page needs to ask of every item without comparing each against all the others: which rects reach a line, in the order they were listed. A rect taller than many bands, or one whose edges are not finite numbers, is offered to every query.
#[derive(Clone, Debug, Default)]
pub struct Bands {
    rows: HashMap<i64, Vec<usize>>,
    everywhere: Vec<usize>,
}

impl Bands {
    /// Files `rects`, each under its place in the list.
    pub fn new(rects: impl IntoIterator<Item = Rect>) -> Self {
        let mut bands = Self::default();
        for (index, rect) in rects.into_iter().enumerate() {
            match span(rect.y, rect.y + rect.height) {
                Some((first, last)) if last - first < MOST_BANDS => {
                    for band in first..=last {
                        bands.rows.entry(band).or_default().push(index);
                    }
                }
                _ => bands.everywhere.push(index),
            }
        }
        bands
    }

    /// Every rect whose span from top to bottom meets the one from `top` to `bottom`, and perhaps some near it that do not, each once, in the order they were listed. A caller tests each for what it asks.
    pub fn crossing(&self, top: f32, bottom: f32) -> Vec<usize> {
        let Some((first, last)) = span(top.min(bottom), top.max(bottom)) else {
            return (0..self.len_hint()).collect();
        };
        let mut found: Vec<usize> = self.everywhere.clone();
        if last - first < MOST_BANDS {
            for band in first..=last {
                found.extend(self.rows.get(&band).into_iter().flatten());
            }
        } else {
            found.extend(self.rows.values().flatten());
        }
        found.sort_unstable();
        found.dedup();
        found
    }

    /// [`crossing`](Self::crossing) a single line at `y`.
    pub fn at(&self, y: f32) -> Vec<usize> {
        self.crossing(y, y)
    }

    /// One past the highest place filed: what a query that cannot be answered by bands offers.
    fn len_hint(&self) -> usize {
        self.rows
            .values()
            .flatten()
            .chain(&self.everywhere)
            .max()
            .map_or(0, |last| last + 1)
    }
}

/// The first and last band a span from `top` to `bottom` crosses, or `None` where either edge is not a finite number.
fn span(top: f32, bottom: f32) -> Option<(i64, i64)> {
    if !top.is_finite() || !bottom.is_finite() {
        return None;
    }
    let band = |y: f32| (f64::from(y) / f64::from(BAND)).floor() as i64;
    Some((band(top), band(bottom.max(top))))
}

#[cfg(test)]
#[path = "bands.test.rs"]
mod tests;
