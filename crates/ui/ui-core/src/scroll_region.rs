//! Where a node actually *is* on screen, once the scroll viewports around it are accounted for.
//!
//! Scrolling is a render transform, not a relayout: a scroll area moves its content by rewriting a matrix and
//! leaves the layout tree exactly where it was. That makes `absolute_rect` — which reads laid-out positions —
//! report where a node *would* be at scroll zero, which is wrong for anything positioned against the node's
//! visible spot. Anchored overlays are the case that matters: a dropdown opened from a trigger scrolled 200px
//! down would otherwise appear 200px below its button.
//!
//! Each scroll area registers its content subtree here with the offset signals driving it, and
//! [`visible_rect`] subtracts every registered offset whose subtree contains the node — so nested scrolls
//! compose without either of them knowing about the other.

use std::cell::RefCell;
use std::mem::ManuallyDrop;

use geometry_core::Rect;
use layout_core::NodeId;
use reactive_core::RwSignal;

use crate::context::{absolute_rect, is_descendant_of};

/// Identifies one registration, so a scroll area can withdraw exactly its own on drop.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ScrollRegionId(u64);

struct ScrollRegion {
    id: ScrollRegionId,
    /// Root of the scrolled subtree — the scroll's *content* node, not its viewport leaf. The content is laid
    /// out as its own root rather than as a child of the viewport, so the viewport is not its ancestor.
    content: NodeId,
    offset_x: RwSignal<f32>,
    offset_y: RwSignal<f32>,
}

// ManuallyDrop keeps these TLS slots trivially-destructible: registering a TLS destructor from a hot-reloaded dylib would make dlclose unsafe (same constraint as `dismiss` and `named_overlay`).
thread_local! {
    static REGIONS: ManuallyDrop<RefCell<Vec<ScrollRegion>>> = ManuallyDrop::new(RefCell::new(Vec::new()));
    static NEXT_ID: ManuallyDrop<RefCell<u64>> = ManuallyDrop::new(RefCell::new(0));
}

/// Registers `content` as a scrolled subtree displaced by `(offset_x, offset_y)`.
pub fn register_scroll_region(
    content: NodeId,
    offset_x: RwSignal<f32>,
    offset_y: RwSignal<f32>,
) -> ScrollRegionId {
    let id = NEXT_ID.with(|n| {
        let mut n = n.borrow_mut();
        *n += 1;
        ScrollRegionId(*n)
    });
    REGIONS.with(|regions| {
        regions.borrow_mut().push(ScrollRegion {
            id,
            content,
            offset_x,
            offset_y,
        })
    });
    id
}

/// Withdraws a registration. A no-op for an id already withdrawn.
pub fn unregister_scroll_region(id: ScrollRegionId) {
    REGIONS.with(|regions| regions.borrow_mut().retain(|r| r.id != id));
}

/// The rect `node` occupies on screen: its laid-out window-absolute rect shifted by the current offset of
/// every scroll viewport it sits inside. `None` when the node has not been laid out under a window root.
///
/// Offsets are read without subscribing, because this answers "where is it right now" for a caller that is
/// positioning something at that moment (an overlay being opened), not one that wants to follow the scroll.
pub fn visible_rect(node: NodeId) -> Option<Rect> {
    let mut rect = absolute_rect(node)?;
    REGIONS.with(|regions| {
        for region in regions.borrow().iter() {
            if is_descendant_of(node, region.content) {
                rect.x -= region.offset_x.peek();
                rect.y -= region.offset_y.peek();
            }
        }
    });
    Some(rect)
}

#[cfg(test)]
mod tests {
    use layout_core::{AvailableSpace, LayoutStyle, SizeDimension};
    use reactive_core::signal;

    use super::*;
    use crate::container::Container;
    use crate::context::{compute_layout, new_container, new_leaf, reset_layout_runtime};
    use crate::layout_item::LayoutItem;

    fn reset() {
        REGIONS.with(|regions| regions.borrow_mut().clear());
    }

    /// A trigger inside a scrolled subtree must report where it is *drawn*, not where it was laid out — the
    /// whole reason an anchored dropdown was landing under the wrong place.
    #[test]
    fn a_scrolled_node_reports_its_on_screen_position() {
        reset_layout_runtime();
        reset();
        let (trigger, _r) = new_leaf(LayoutStyle::new().width(50.0).height(20.0)).unwrap();
        let content = new_container(
            LayoutStyle::new()
                .flex_column()
                .width(SizeDimension::Percent(1.0)),
            &[trigger],
        )
        .unwrap();
        compute_layout(
            content,
            AvailableSpace::Definite(300.0),
            AvailableSpace::Definite(300.0),
        )
        .unwrap();

        let laid_out = absolute_rect(trigger).unwrap();
        assert_eq!(
            visible_rect(trigger),
            Some(laid_out),
            "unscrolled: identical"
        );

        let (x, y) = (signal(0.0f32), signal(120.0f32));
        let id = register_scroll_region(content, x.clone(), y.clone());
        let shifted = visible_rect(trigger).unwrap();
        assert_eq!(
            shifted.y,
            laid_out.y - 120.0,
            "scrolled down 120px, so it is drawn 120px higher"
        );
        assert_eq!(shifted.x, laid_out.x);
        assert_eq!(
            shifted.width, laid_out.width,
            "scrolling moves a node, it does not resize it"
        );

        // Scrolling back restores the laid-out position, and withdrawing stops the adjustment entirely.
        y.set(0.0);
        assert_eq!(visible_rect(trigger), Some(laid_out));
        y.set(80.0);
        unregister_scroll_region(id);
        assert_eq!(visible_rect(trigger), Some(laid_out));
    }

    /// Nested scrolls compose: each contributes its own offset, without either knowing about the other.
    #[test]
    fn nested_scroll_offsets_accumulate() {
        reset_layout_runtime();
        reset();
        let inner_leaf =
            Container::new(LayoutStyle::new().width(10.0).height(10.0), vec![]).unwrap();
        let trigger = inner_leaf.layout_node();
        let inner = new_container(LayoutStyle::new().flex_column(), &[trigger]).unwrap();
        let outer = new_container(LayoutStyle::new().flex_column(), &[inner]).unwrap();
        compute_layout(
            outer,
            AvailableSpace::Definite(300.0),
            AvailableSpace::Definite(300.0),
        )
        .unwrap();
        let laid_out = absolute_rect(trigger).unwrap();

        register_scroll_region(outer, signal(0.0), signal(30.0));
        register_scroll_region(inner, signal(5.0), signal(7.0));
        let shifted = visible_rect(trigger).unwrap();
        assert_eq!(shifted.y, laid_out.y - 37.0);
        assert_eq!(shifted.x, laid_out.x - 5.0);
    }

    /// A node outside a registered subtree is untouched by that scroll — otherwise every overlay in the app
    /// would shift whenever any unrelated pane scrolled.
    #[test]
    fn a_node_outside_the_region_is_unaffected() {
        reset_layout_runtime();
        reset();
        let (inside, _a) = new_leaf(LayoutStyle::new().width(10.0).height(10.0)).unwrap();
        let (outside, _b) = new_leaf(LayoutStyle::new().width(10.0).height(10.0)).unwrap();
        let scrolled = new_container(LayoutStyle::new().flex_column(), &[inside]).unwrap();
        let root = new_container(LayoutStyle::new().flex_column(), &[scrolled, outside]).unwrap();
        compute_layout(
            root,
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(200.0),
        )
        .unwrap();

        let outside_before = absolute_rect(outside).unwrap();
        register_scroll_region(scrolled, signal(0.0), signal(50.0));
        assert_eq!(visible_rect(outside), Some(outside_before));
        assert_eq!(
            visible_rect(inside).unwrap().y,
            absolute_rect(inside).unwrap().y - 50.0
        );
    }
}
