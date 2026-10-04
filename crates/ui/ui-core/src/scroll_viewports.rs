//! Which scroll shows which content: the way from any node to the viewports it scrolls in, whatever order the tree was built in, the surface's primary scroll for whoever reads it from outside its content, and the layers fixed over it.
//!
//! A scroll area lays its content out as a root of its own, so a climb up the layout tree from a node inside it ends at that content root and never reaches the area. This records the missing link, per surface because node ids are minted per surface.

use layout_core::NodeId;
use reactive_core::{RwSignal, signal};
use rustc_hash::FxHashMap;

use crate::scroll_area::ScrollViewport;

reactive_core::surface_local! {
    slot VIEWPORTS: Viewports = Viewports::default();
    access with_viewports, with_viewports_ref;
    context ViewportsContext, ViewportsGuard;
}

struct Viewports {
    by_content: FxHashMap<NodeId, (ScrollViewport, u64)>,
    made: u64,
    /// The page whose scroll is the surface's primary scroll, with the stamp of the claim that made it so.
    primary: RwSignal<Option<(u64, ScrollViewport)>>,
    /// The boxes of every layer fixed over the surface, with the stamp of the layer's registration.
    layers: RwSignal<Vec<(u64, Vec<NodeId>)>>,
}

impl Default for Viewports {
    fn default() -> Self {
        Self {
            by_content: FxHashMap::default(),
            made: 0,
            primary: signal(None),
            layers: signal(Vec::new()),
        }
    }
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

/// The surface's primary scroll, read reactively from anywhere: the viewport of the page that holds it, or `None` while no page does.
///
/// Where [`use_scroll_viewport`](crate::use_scroll_viewport) answers only inside a scroll area's builder, this answers outside the page's content too — a bar fixed over the page, an app shell, an effect at the root — and follows the page across a rebuild: whoever reads it is run again when a page takes the primary scroll or lets it go. The viewport's own offset, rect and [`progress`](ScrollViewport::progress) are signals of their own, so reading them here follows the scroll as well.
pub fn use_primary_scroll() -> Option<ScrollViewport> {
    let primary = with_viewports_ref(|v| v.primary);
    primary.with(|primary| primary.as_ref().map(|(_, viewport)| viewport.clone()))
}

/// Makes `viewport` what [`use_primary_scroll`] answers until the returned publication drops. The newest wins, as with the platform's claim: a page rebuilt on the same surface is built before the old one is dropped.
pub(crate) fn publish_primary(viewport: ScrollViewport) -> PrimaryPublication {
    let (primary, stamp) = with_viewports(|v| {
        v.made += 1;
        (v.primary, v.made)
    });
    primary.set(Some((stamp, viewport)));
    PrimaryPublication { primary, stamp }
}

/// Holds a page's publication as the primary scroll; dropping it withdraws the page, unless a newer one has already taken its place.
pub(crate) struct PrimaryPublication {
    primary: RwSignal<Option<(u64, ScrollViewport)>>,
    stamp: u64,
}

impl Drop for PrimaryPublication {
    fn drop(&mut self) {
        if !self.primary.is_alive() {
            return;
        }
        let current = self.primary.peek_with(|primary| {
            primary
                .as_ref()
                .is_some_and(|(stamp, _)| *stamp == self.stamp)
        });
        if current {
            self.primary.set(None);
        }
    }
}

/// Records the boxes of a layer fixed over the surface until the returned registration drops, for [`fixed_layer_boxes`] to answer.
pub(crate) fn register_layer(boxes: Vec<NodeId>) -> LayerRegistration {
    let (layers, stamp) = with_viewports(|v| {
        v.made += 1;
        (v.layers, v.made)
    });
    layers.update(|layers| layers.push((stamp, boxes)));
    LayerRegistration { layers, stamp }
}

/// The boxes of every layer fixed over the surface right now, read reactively: whoever reads it runs again when a layer comes or goes.
pub(crate) fn fixed_layer_boxes() -> Vec<NodeId> {
    let layers = with_viewports_ref(|v| v.layers);
    layers.with(|layers| {
        layers
            .iter()
            .flat_map(|(_, boxes)| boxes.iter().copied())
            .collect()
    })
}

/// Keeps a layer recorded; see [`register_layer`].
pub(crate) struct LayerRegistration {
    layers: RwSignal<Vec<(u64, Vec<NodeId>)>>,
    stamp: u64,
}

impl Drop for LayerRegistration {
    fn drop(&mut self) {
        if self.layers.is_alive() {
            let stamp = self.stamp;
            self.layers
                .update(|layers| layers.retain(|(made, _)| *made != stamp));
        }
    }
}

#[cfg(test)]
#[path = "scroll_viewports_test.rs"]
mod tests;
