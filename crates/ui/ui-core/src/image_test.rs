use crate::context::reset_layout_runtime;
use layout_core::AvailableSpace;

use super::*;
use crate::context::{compute_layout, new_container};
use crate::layout_item::LayoutItem;

#[test]
fn image_without_size_uses_intrinsic_size() {
    reset_layout_runtime();
    let data = Arc::new(ImageData::new(vec![0u8; 40 * 20 * 4], 40, 20));
    let image = Image::new(
        LayoutStyle::new(),
        move || Arc::clone(&data),
        || Raster::Smooth,
        || ObjectFit::Contain,
    )
    .unwrap();
    let root = new_container(
        LayoutStyle::new().flex_column().width(200.0).height(200.0),
        &[image.layout_node()],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();

    let rect = image.leaf.rect.get();
    assert_eq!(rect.width, 40.0);
    assert_eq!(rect.height, 20.0);
}

/// A radius has to reach the render tree even on a fit that overflows nothing, since `Contain` takes the early return that skips the clip entirely.
#[test]
fn a_radius_clips_the_picture_whatever_the_fit() {
    reset_layout_runtime();
    let data = Arc::new(ImageData::new(vec![0u8; 40 * 20 * 4], 40, 20));
    let image = Image::new(
        LayoutStyle::new().width(40.0).height(20.0),
        move || Arc::clone(&data),
        || Raster::Smooth,
        || ObjectFit::Contain,
    )
    .unwrap()
    .with_radius(6.0);

    let radius_of = |node: &RenderNode| -> Option<BorderRadius> {
        let mut current = node;
        loop {
            match current {
                RenderNode::Clip { radius, .. } => return Some(*radius),
                RenderNode::Transform { children, .. } | RenderNode::Layer { children, .. } => {
                    current = children.first()?;
                }
                _ => return None,
            }
        }
    };
    assert_eq!(
        radius_of(&image.view()).map(|r| r.top_left),
        Some(6.0),
        "the clip carries the radius the caller asked for"
    );
}

#[test]
fn image_single_side_derives_aspect() {
    reset_layout_runtime();
    let data = Arc::new(ImageData::new(vec![0u8; 40 * 20 * 4], 40, 20));
    let image = Image::new(
        LayoutStyle::new().width(100.0),
        move || Arc::clone(&data),
        || Raster::Smooth,
        || ObjectFit::Contain,
    )
    .unwrap();
    let root = new_container(
        LayoutStyle::new().flex_column().width(300.0).height(300.0),
        &[image.layout_node()],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(300.0),
        AvailableSpace::Definite(300.0),
    )
    .unwrap();

    let rect = image.leaf.rect.get();
    assert_eq!(rect.width, 100.0);
    assert_eq!(rect.height, 50.0);
}
