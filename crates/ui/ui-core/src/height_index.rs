//! [`HeightIndex`]: where each row of a variable-height list starts, kept as a Fenwick tree so a lookup and an update both cost `O(log n)`.

/// The heights of a column of rows and the running sums over them: which row sits at an offset, where a row starts, and how tall the whole column is, each answered in `O(log n)` however long the list.
///
/// A variable-height [`VirtualList`](crate::VirtualList) asks this on every scroll tick and corrects one row's height every time a row is measured; a plain array of offsets would answer the first in `O(log n)` but pay `O(n)` for the second, which on a hundred thousand rows is what scrolling a freshly built list would cost.
///
/// Sums are kept in `f64`: a hundred thousand rows add up to millions of pixels, where `f32` can no longer tell a row's top from the next pixel down.
#[derive(Clone, Debug, Default)]
pub struct HeightIndex {
    heights: Vec<f32>,
    tree: Vec<f64>,
}

impl HeightIndex {
    /// An index over `heights`, built in `O(n)`. A negative height counts as zero.
    pub fn new(heights: impl IntoIterator<Item = f32>) -> Self {
        let heights: Vec<f32> = heights.into_iter().map(|h| h.max(0.0)).collect();
        let mut tree = vec![0.0f64; heights.len() + 1];
        for (at, &height) in heights.iter().enumerate() {
            let node = at + 1;
            tree[node] += f64::from(height);
            let parent = node + lowest_bit(node);
            if parent < tree.len() {
                tree[parent] += tree[node];
            }
        }
        Self { heights, tree }
    }

    /// `count` rows of `height` each.
    pub fn uniform(count: usize, height: f32) -> Self {
        Self::new(std::iter::repeat_n(height, count))
    }

    /// How many rows there are.
    pub fn len(&self) -> usize {
        self.heights.len()
    }

    /// Whether there are no rows at all.
    pub fn is_empty(&self) -> bool {
        self.heights.is_empty()
    }

    /// The height of row `index`, or `0.0` past the end.
    pub fn height(&self, index: usize) -> f32 {
        self.heights.get(index).copied().unwrap_or(0.0)
    }

    /// Makes row `index` `height` tall, answering how far every row after it moved. Past the end it changes nothing and answers `0.0`.
    pub fn set(&mut self, index: usize, height: f32) -> f32 {
        let Some(slot) = self.heights.get_mut(index) else {
            return 0.0;
        };
        let height = height.max(0.0);
        let delta = height - *slot;
        if delta == 0.0 {
            return 0.0;
        }
        *slot = height;
        let mut node = index + 1;
        while node < self.tree.len() {
            self.tree[node] += f64::from(delta);
            node += lowest_bit(node);
        }
        delta
    }

    /// Replaces the `remove` rows starting at `at` with rows of the heights in `insert`, the rows after them shifting to fit. Both are clamped to the end.
    ///
    /// Only the sums from `at` onward are worked out again, and a replacement of as many rows as it removes touches just those rows, so an edit near the end of a long list costs a fraction of building the index over.
    pub fn splice(&mut self, at: usize, remove: usize, insert: impl IntoIterator<Item = f32>) {
        let at = at.min(self.len());
        let end = at.saturating_add(remove).min(self.len());
        let insert: Vec<f32> = insert.into_iter().map(|h| h.max(0.0)).collect();
        let levels = (usize::BITS - self.len().leading_zeros()) as usize;
        if insert.len() == end - at && insert.len() * levels < self.len() - at {
            for (offset, height) in insert.into_iter().enumerate() {
                self.set(at + offset, height);
            }
            return;
        }
        self.heights.splice(at..end, insert);
        self.tree.resize(self.heights.len() + 1, 0.0);
        for node in at + 1..self.tree.len() {
            let mut sum = f64::from(self.heights[node - 1]);
            let mut child = 1;
            while child < lowest_bit(node) {
                sum += self.tree[node - child];
                child <<= 1;
            }
            self.tree[node] = sum;
        }
    }

    /// Where row `index` starts: the combined height of every row before it. `index == len()` is the end of the last row; anything past it is clamped there.
    pub fn offset_of(&self, index: usize) -> f32 {
        self.prefix(index) as f32
    }

    /// The combined height of rows `start..end`, summed before it is rounded to `f32` so a span deep in a long list is as exact as one at its top.
    pub fn span(&self, start: usize, end: usize) -> f32 {
        (self.prefix(end) - self.prefix(start)).max(0.0) as f32
    }

    /// The combined height of every row.
    pub fn total(&self) -> f32 {
        self.offset_of(self.len())
    }

    fn prefix(&self, index: usize) -> f64 {
        let mut node = index.min(self.len());
        let mut sum = 0.0f64;
        while node > 0 {
            sum += self.tree[node];
            node -= lowest_bit(node);
        }
        sum
    }

    /// The row that `offset` falls inside, a row owning its top edge and not its bottom one. An offset above the first row is the first row, one below the last is the last, and an empty index answers `0`.
    pub fn row_at(&self, offset: f32) -> usize {
        let count = self.len();
        if count == 0 {
            return 0;
        }
        let mut remaining = f64::from(offset).max(0.0);
        let mut at = 0;
        let mut step = 1usize << (usize::BITS - 1 - count.leading_zeros());
        while step > 0 {
            let next = at + step;
            if next <= count && self.tree[next] <= remaining {
                at = next;
                remaining -= self.tree[next];
            }
            step >>= 1;
        }
        at.min(count - 1)
    }

    /// The rows worth building for a viewport `viewport_height` tall scrolled to `offset`, with `overscan` px built beyond each edge: a half-open `[first, last)`.
    ///
    /// [`visible_window`](crate::visible_window) for rows of differing heights. Like it, a viewport not yet laid out answers every row, so a first frame shows the list rather than nothing.
    pub fn window(&self, offset: f32, viewport_height: f32, overscan: f32) -> (usize, usize) {
        let count = self.len();
        if viewport_height <= 0.0 || count == 0 {
            return (0, count);
        }
        let overscan = overscan.max(0.0);
        let first = self.row_at(offset - overscan);
        let last = self.row_at(offset + viewport_height + overscan) + 1;
        (first, last.min(count))
    }
}

fn lowest_bit(node: usize) -> usize {
    node & node.wrapping_neg()
}

#[cfg(test)]
#[path = "height_index_test.rs"]
mod tests;
