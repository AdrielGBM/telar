use geometry_core::Rect;
use layout_core::{LayoutError, LayoutStyle, NodeId};
use reactive_core::RwSignal;
use ui_tree::Component;

use crate::context::{WidgetCtx, new_container, track_layout};
use crate::layout_leaf::LayoutLeaf;

pub(crate) type TrackedChildren = Vec<(Box<dyn LayoutItem>, Option<RwSignal<Rect>>)>;

pub(crate) trait LeafWidget {
    fn layout_leaf(&self) -> &LayoutLeaf;
}

pub trait LayoutItem: Component {
    fn layout_node(&self) -> NodeId;
}

impl<T: LeafWidget + Component> LayoutItem for T {
    fn layout_node(&self) -> NodeId {
        self.layout_leaf().node
    }
}

// pub so the `children!` macro can call it from any crate without naming the module
pub fn box_item(item: impl LayoutItem + 'static) -> Box<dyn LayoutItem> {
    Box::new(item)
}

pub(crate) fn register_container(
    ctx: &mut WidgetCtx,
    layout_style: LayoutStyle,
    children: Vec<Box<dyn LayoutItem>>,
) -> Result<(NodeId, RwSignal<Rect>, TrackedChildren), LayoutError> {
    let child_nodes = children.iter().map(|c| c.layout_node()).collect::<Vec<_>>();
    let node = new_container(ctx, layout_style, &child_nodes)?;
    let rect = track_layout(ctx, node).expect("new_container always registers a signal");
    let children = children
        .into_iter()
        .map(|c| {
            let rect = track_layout(ctx, c.layout_node());
            (c, rect)
        })
        .collect();
    Ok((node, rect, children))
}

/// Implements `LeafWidget` for a struct that has a `leaf: LayoutLeaf` field.
#[macro_export]
macro_rules! impl_leaf_widget {
    ($struct:ident) => {
        impl $crate::layout_item::LeafWidget for $struct {
            fn layout_leaf(&self) -> &$crate::layout_leaf::LayoutLeaf {
                &self.leaf
            }
        }
    };
}
