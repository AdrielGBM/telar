//! [`Image`]: a bitmap leaf, sized by `object-fit` and clipped to its corner radius.

use std::sync::Arc;

use geometry_core::{ObjectFit, Rect};
use layout_core::{LayoutError, LayoutStyle};
use platform_core::Event;
use renderer_core::{BorderRadius, DrawCommand, ImageData, ImageFill, ImageSlice, Raster};
use ui_tree::{Component, EventResult, RenderNode};

use crate::impl_leaf_widget;
use crate::layout_leaf::LayoutLeaf;

/// A bitmap leaf, sized by `object-fit` and clipped to its corner radius.
pub struct Image {
    data: Box<dyn Fn() -> Arc<ImageData>>,
    leaf: LayoutLeaf,
    raster: Box<dyn Fn() -> Raster>,
    fit: Box<dyn Fn() -> ObjectFit>,
    slice: Option<Box<dyn Fn() -> ImageSlice>>,
    radius: BorderRadius,
}

impl Image {
    pub fn new(
        layout_style: LayoutStyle,
        data_fn: impl Fn() -> Arc<ImageData> + 'static,
        raster_fn: impl Fn() -> Raster + 'static,
        fit_fn: impl Fn() -> ObjectFit + 'static,
    ) -> Result<Self, LayoutError> {
        // A side left at `auto` falls back to the intrinsic size, a single px side derives the other from the intrinsic aspect ratio, and a percent side is left untouched.
        let layout_style = crate::layout_leaf::resolve_intrinsic_size(layout_style, || {
            let d = data_fn();
            (d.width as f32, d.height as f32)
        });

        let leaf = LayoutLeaf::register(layout_style)?;
        Ok(Self {
            data: Box::new(data_fn),
            leaf,
            raster: Box::new(raster_fn),
            fit: Box::new(fit_fn),
            slice: None,
            radius: BorderRadius::zero(),
        })
    }

    /// Draws the picture nine-sliced across the whole box: its corners at their own size, its edges stretched along the sides and its middle over the rest, the way a frame or a panel skin is drawn. Takes the place of the fit, which has nothing left to place.
    pub fn with_slice(mut self, slice_fn: impl Fn() -> ImageSlice + 'static) -> Self {
        self.slice = Some(Box::new(slice_fn));
        self
    }

    /// Rounds the picture's own corners.
    ///
    /// A `StyledContainer` around it cannot do this — its radius rounds the *fill* it paints, and a bitmap child draws over that — so a thumbnail, an avatar or a cover in a rounded UI had no way to be anything but a square. The clip primitive already carries a radius; this is the widget passing one through.
    pub fn with_radius(self, radius: f32) -> Self {
        self.with_border_radius(BorderRadius::all(radius))
    }

    /// Rounds each corner separately, for a picture that meets an edge on one side only.
    pub fn with_border_radius(mut self, radius: BorderRadius) -> Self {
        // A negative corner is not a shape the clip can answer, and it reaches here from a `radius:` the author typed.
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
        let ((content, clip), fill) = match (&self.slice, (self.fit)()) {
            (Some(slice), _) => ((r_local, false), ImageFill::Slice(slice())),
            (None, fit) => (
                geometry_core::fit_rect((data.width as f32, data.height as f32), r_local, fit),
                match fit {
                    ObjectFit::Tile { scale } => ImageFill::Tile { scale },
                    _ => ImageFill::Stretch,
                },
            ),
        };
        let image = RenderNode::Primitive(DrawCommand::Image {
            data,
            rect: content,
            raster: (self.raster)(),
            fill,
        });
        // Cover overflows the box. The renderer maps clip rects through the active matrix, so a local (0,0,w,h) clip composes with this widget's transform and any scroll. A radius clips a `Contain` fit too.
        let node = if clip || !self.radius.is_zero() {
            RenderNode::clip(r_local, self.radius, [image])
        } else {
            image
        };
        self.leaf
            .at_layout_position_as(renderer_core::Semantics::drawing, node)
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
#[path = "image_test.rs"]
mod tests;
