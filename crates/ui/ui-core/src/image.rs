use std::sync::Arc;

use geometry_core::{ObjectFit, Rect};
use layout_core::{LayoutError, LayoutStyle};
use platform_core::Event;
use renderer_core::{BorderRadius, DrawCommand, ImageData, ImageFilter};
use ui_tree::{Component, EventResult, RenderNode};

use crate::impl_leaf_widget;
use crate::layout_leaf::LayoutLeaf;

pub struct Image {
    data: Box<dyn Fn() -> Arc<ImageData>>,
    leaf: LayoutLeaf,
    filter: Box<dyn Fn() -> ImageFilter>,
    fit: Box<dyn Fn() -> ObjectFit>,
    radius: BorderRadius,
}

impl Image {
    pub fn new(
        layout_style: LayoutStyle,
        data_fn: impl Fn() -> Arc<ImageData> + 'static,
        filter_fn: impl Fn() -> ImageFilter + 'static,
        fit_fn: impl Fn() -> ObjectFit + 'static,
    ) -> Result<Self, LayoutError> {
        // A side left at `auto` falls back to the bitmap's intrinsic size; a single px side derives the other from the intrinsic aspect ratio; a percent side is left untouched.
        let layout_style = crate::layout_leaf::resolve_intrinsic_size(layout_style, || {
            let d = data_fn();
            (d.width as f32, d.height as f32)
        });

        let leaf = LayoutLeaf::register(layout_style)?;
        Ok(Self {
            data: Box::new(data_fn),
            leaf,
            filter: Box::new(filter_fn),
            fit: Box::new(fit_fn),
            radius: BorderRadius::zero(),
        })
    }

    /// Rounds the picture's own corners.
    ///
    /// A `StyledContainer` around it cannot do this — its radius rounds the *fill* it paints, and a bitmap child
    /// draws over that — so a thumbnail, an avatar or a cover in a rounded UI had no way to be anything but a
    /// square. The clip primitive already carries a radius; this is the widget passing one through.
    pub fn with_radius(self, radius: f32) -> Self {
        self.with_border_radius(BorderRadius::all(radius))
    }

    /// Rounds each corner separately, for a picture that meets an edge on one side only.
    pub fn with_border_radius(mut self, radius: BorderRadius) -> Self {
        // A negative corner is not a shape the clip can answer, and it reaches here from a `radius:` the
        // author typed rather than from anything the widget controls.
        self.radius = BorderRadius {
            top_left: radius.top_left.max(0.0),
            top_right: radius.top_right.max(0.0),
            bottom_right: radius.bottom_right.max(0.0),
            bottom_left: radius.bottom_left.max(0.0),
        };
        self
    }
}

impl Component for Image {
    fn view(&self) -> RenderNode {
        let r = self.leaf.rect.get();
        let r_local = Rect {
            x: 0.0,
            y: 0.0,
            width: r.width,
            height: r.height,
        };
        let data = (self.data)();
        let (content, clip) = geometry_core::fit_rect(
            (data.width as f32, data.height as f32),
            r_local,
            (self.fit)(),
        );
        let image = RenderNode::Primitive(DrawCommand::Image {
            data,
            rect: content,
            filter: (self.filter)(),
        });
        // Cover overflows the box; clip it to the local box. The renderer maps clip rects through the active matrix, so a local (0,0,w,h) clip composes with this widget's layout transform and any scroll. A radius is the other reason to clip, and it applies to a `Contain` fit that overflows nothing too.
        let node = if clip || !self.radius.is_zero() {
            RenderNode::clip(r_local, self.radius, [image])
        } else {
            image
        };
        self.leaf.at_layout_position(node)
    }

    fn on_event(&mut self, _event: &Event) -> EventResult {
        EventResult::Ignored
    }

    fn debug_name(&self) -> &'static str {
        "Image"
    }
}

impl_leaf_widget!(Image);

#[cfg(test)]
mod tests {
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
            || ImageFilter::Linear,
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

    /// A radius has to reach the render tree even on a fit that overflows nothing, since `Contain` takes the
    /// early return that skips the clip entirely.
    #[test]
    fn a_radius_clips_the_picture_whatever_the_fit() {
        reset_layout_runtime();
        let data = Arc::new(ImageData::new(vec![0u8; 40 * 20 * 4], 40, 20));
        let image = Image::new(
            LayoutStyle::new().width(40.0).height(20.0),
            move || Arc::clone(&data),
            || ImageFilter::Linear,
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
        // Width pinned, height auto → height follows the 2:1 intrinsic aspect ratio.
        let image = Image::new(
            LayoutStyle::new().width(100.0),
            move || Arc::clone(&data),
            || ImageFilter::Linear,
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
}
