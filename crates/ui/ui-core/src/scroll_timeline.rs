//! Progress driven by scrolling: how far a box has travelled through the view of the scroll it sits in, as a reactive `0.0..=1.0` a [`Timeline`](motion_core::Timeline) samples.
//!
//! The same numbers on every target, because they are computed from Telar's own layout and the scroll's offset, which every target keeps: the document's scroll on web-dom, a scroll area drawn at its offset everywhere else.

use layout_core::NodeId;
use reactive_core::{ReadSignal, RwSignal, effect, signal};

use crate::Axis;
use crate::context::track_layout;
use crate::layout_item::LayoutItem;
use crate::scroll_area::ScrollViewport;
use crate::scroll_viewports::enclosing_scroll_viewport;

/// Which stretch of a box's passage through the view a progress runs over, as CSS names view-timeline ranges.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ViewRange {
    /// From the box's start edge entering the view to its end edge leaving it: the whole passage.
    #[default]
    Cover,
    /// While the box is wholly in view, or, for a box taller than the view, while it fills it. A scene whose stage sticks inside a taller track runs over this range of the track.
    Contain,
    /// From the box starting to enter the view until it is wholly in it (or fills it).
    Entry,
    /// From the box starting to leave the view until it has left it.
    Exit,
}

impl ViewRange {
    /// The word `.rsx` spells this range with, and back.
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "cover" => Self::Cover,
            "contain" => Self::Contain,
            "entry" => Self::Entry,
            "exit" => Self::Exit,
            _ => return None,
        })
    }
}

/// How far through `range` a box is, along one axis: its start `start` and size `size` in the scroll's content, a view `view` long, scrolled to `offset`. Clamped to `0.0..=1.0`; a range of no length is a step at its start.
pub fn range_progress(range: ViewRange, start: f32, size: f32, view: f32, offset: f32) -> f32 {
    let end = start + size;
    let entering = start - view;
    let (contain_from, contain_to) = ((end - view).min(start), (end - view).max(start));
    let (from, to) = match range {
        ViewRange::Cover => (entering, end),
        ViewRange::Contain => (contain_from, contain_to),
        ViewRange::Entry => (entering, contain_from),
        ViewRange::Exit => (contain_to, end),
    };
    if to - from <= f32::EPSILON {
        return if offset >= from { 1.0 } else { 0.0 };
    }
    ((offset - from) / (to - from)).clamp(0.0, 1.0)
}

/// How far `node` is through `range` of its passage through the view of the scroll it sits in, top to bottom. `0.0` while it sits in no scroll or has not been laid out.
///
/// The scroll is found from the node itself, nearest first, so it works whatever order the tree was built in and on the page's own scroll as on any scroll area. Kept while the calling scope lives.
pub fn scroll_progress(node: NodeId, range: ViewRange) -> ReadSignal<f32> {
    scroll_progress_along(node, range, Axis::Vertical)
}

/// [`scroll_progress`] along `axis`, for a scroll that moves across.
pub fn scroll_progress_along(node: NodeId, range: ViewRange, axis: Axis) -> ReadSignal<f32> {
    let progress = signal(0.0f32);
    follow_progress(node, range, axis, progress);
    progress.read_only()
}

fn follow_progress(node: NodeId, range: ViewRange, axis: Axis, into: RwSignal<f32>) {
    effect(move || {
        let Some(rect) = track_layout(node) else {
            return;
        };
        let item = rect.get();
        let Some(viewport) = enclosing_scroll_viewport(node) else {
            return;
        };
        let view = viewport.rect().get();
        let (offset_x, offset_y) = viewport.offset();
        let now = match axis {
            Axis::Vertical => {
                range_progress(range, item.y, item.height, view.height, offset_y.get())
            }
            Axis::Horizontal => {
                range_progress(range, item.x, item.width, view.width, offset_x.get())
            }
        };
        if into.peek() != now {
            into.set(now);
        }
    });
}

/// The scroll viewport whose content is being built right now, for content that reads or moves the scroll it is in. `None` outside a scroll area's builder; a node that already exists finds its scroll with [`enclosing_scroll_viewport`].
pub fn use_scroll_viewport() -> Option<ScrollViewport> {
    reactive_core::with_context::<ScrollViewport, _>(Clone::clone)
}

/// Writes a box's scroll progress into a signal the author owns: what `view_progress:$p view_range:contain` does in `.rsx`. Every widget with a layout node takes it.
pub trait ScrollLinked: LayoutItem + Sized {
    /// Keeps `into` at how far this box is through `range` of its passage through the view; see [`scroll_progress`].
    fn view_progress(self, range: ViewRange, into: RwSignal<f32>) -> Self {
        follow_progress(self.layout_node(), range, Axis::Vertical, into);
        self
    }
}

impl<T: LayoutItem> ScrollLinked for T {}

#[cfg(test)]
#[path = "scroll_timeline_test.rs"]
mod tests;
