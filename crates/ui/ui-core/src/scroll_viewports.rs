//! Which scroll shows which content: the way from any node to the viewports it scrolls in, whatever order the tree was built in.
//!
//! A scroll area lays its content out as a root of its own, so a climb up the layout tree from a node inside it ends at that content root and never reaches the area. This records the missing link, per surface because node ids are minted per surface.

use layout_core::NodeId;
use rustc_hash::FxHashMap;

use crate::scroll_area::ScrollViewport;

reactive_core::surface_local! {
    slot VIEWPORTS: Viewports = Viewports::default();
    access with_viewports, with_viewports_ref;
    context ViewportsContext, ViewportsGuard;
}

#[derive(Default)]
struct Viewports {
    by_content: FxHashMap<NodeId, (ScrollViewport, u64)>,
    made: u64,
}

/// Records that `viewport` shows the content rooted at `content`, until the owner registering it is disposed.
pub(crate) fn register(content: NodeId, viewport: ScrollViewport) {
    let generation = with_viewports(|v| {
        v.made += 1;
        v.by_content.insert(content, (viewport, v.made));
        v.made
    });
    reactive_core::on_cleanup(move || {
        with_viewports(|v| {
            if v.by_content
                .get(&content)
                .is_some_and(|(_, made)| *made == generation)
            {
                v.by_content.remove(&content);
            }
        });
    });
}

/// The scroll viewport `node` scrolls in, nearest first: `None` for a node that no scroll shows, such as one laid out straight against the surface.
pub fn enclosing_scroll_viewport(node: NodeId) -> Option<ScrollViewport> {
    let root = layout_reactive::ancestors(node).last()?;
    with_viewports_ref(|v| {
        v.by_content
            .get(&root)
            .map(|(viewport, _)| viewport.clone())
    })
}

/// Every scroll viewport `node` scrolls in, nearest first, out to the surface: a node in a scroll area that is itself inside a page is in both.
pub fn scroll_viewports_of(node: NodeId) -> Vec<ScrollViewport> {
    let mut chain: Vec<ScrollViewport> = Vec::new();
    let mut at = node;
    while let Some(viewport) = enclosing_scroll_viewport(at) {
        if chain.iter().any(|seen| seen.area() == viewport.area()) {
            break;
        }
        at = viewport.area();
        chain.push(viewport);
    }
    chain
}

#[cfg(test)]
#[path = "scroll_viewports_test.rs"]
mod tests;
