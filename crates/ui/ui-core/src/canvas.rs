use crate::impl_leaf_widget;
use crate::layout_leaf::LayoutLeaf;
use geometry_core::Rect;
use layout_core::{LayoutError, LayoutStyle};
use platform_core::Event;
use renderer_core::TextStyle;
use ui_tree::{Component, EventResult, RenderNode};

/// Where a canvas gets the paint for its artwork: from itself, or from the text the tree above it declared.
enum Artwork {
    Fixed(Box<dyn Fn(Rect) -> RenderNode>),
    Inheriting(Box<dyn Fn(Rect, TextStyle) -> RenderNode>),
}

pub struct Canvas {
    leaf: LayoutLeaf,
    draw: Artwork,
}

impl Canvas {
    pub fn new(
        layout_style: LayoutStyle,
        draw_fn: impl Fn(Rect) -> RenderNode + 'static,
    ) -> Result<Self, LayoutError> {
        let leaf = LayoutLeaf::register(layout_style)?;
        Ok(Self {
            leaf,
            draw: Artwork::Fixed(Box::new(draw_fn)),
        })
    }

    /// A canvas whose artwork is handed the text style the tree above it declared.
    ///
    /// For the glyphs an interface draws rather than spells — a caret, a tick, a chevron — because a font
    /// cannot be relied on to carry them at the size and weight the label beside them is set in. Drawn, they
    /// stop being text and lose the cascade with it: the caret on a `select` stayed the theme's ink in a
    /// region that had declared its own, while the label it points at followed. This is CSS's `currentColor`,
    /// for the shapes a face does not supply.
    pub fn declaring(
        layout_style: LayoutStyle,
        draw_fn: impl Fn(Rect, TextStyle) -> RenderNode + 'static,
    ) -> Result<Self, LayoutError> {
        let leaf = LayoutLeaf::register(layout_style)?;
        Ok(Self {
            leaf,
            draw: Artwork::Inheriting(Box::new(draw_fn)),
        })
    }
}

impl Canvas {
    pub fn with_intrinsic_height(
        height: f32,
        draw_fn: impl Fn(geometry_core::Rect) -> ui_tree::RenderNode + 'static,
    ) -> Result<Self, layout_core::LayoutError> {
        Self::new(layout_core::LayoutStyle::new().height(height), draw_fn)
    }
}

impl_leaf_widget!(Canvas);

impl Component for Canvas {
    fn view(&self) -> RenderNode {
        let r = self.leaf.rect.get();
        // A Canvas closure draws at fixed coordinates that ignore the layout rect, so a collapsed rect
        // (e.g. a section hidden via `display:none`) would still paint over other content. Draw nothing.
        if r.width <= 0.0 || r.height <= 0.0 {
            return RenderNode::Empty;
        }
        // The closure draws in local space (at_layout_position translates the output), so it gets a zero-origin rect — passing the absolute layout rect would double-offset anything derived from rect.x/y.
        let local = Rect {
            x: 0.0,
            y: 0.0,
            width: r.width,
            height: r.height,
        };
        let inner = match &self.draw {
            Artwork::Fixed(draw) => draw(local),
            Artwork::Inheriting(draw) => {
                draw(local, crate::inherit::inherited_text_style(self.leaf.node))
            }
        };
        self.leaf.at_layout_position(inner)
    }

    fn on_event(&mut self, _event: &Event) -> EventResult {
        EventResult::Ignored
    }

    fn debug_name(&self) -> &'static str {
        "Canvas"
    }
}

#[cfg(test)]
mod tests {
    use crate::context::reset_layout_runtime;
    use std::cell::Cell;
    use std::rc::Rc;

    use layout_core::AvailableSpace;
    use renderer_core::{Color, DrawCommand, Paint, RectStyle, ShapeStyle};

    use super::*;
    use crate::context::{compute_layout, new_container};
    use crate::layout_item::LayoutItem;

    // `draw` must be re-invoked on every `view()`, not cached from construction — a `$signal` colour
    // read inside it (the reactive path the transpiler now clones into a canvas child's `fill`/`stroke`)
    // would otherwise freeze at whatever value was current when the closure was built.
    #[test]
    fn draw_closure_is_re_read_each_view_and_recolors() {
        let color = Rc::new(Cell::new(Color::RED));
        let color_read = color.clone();
        reset_layout_runtime();
        let canvas = Canvas::new(LayoutStyle::new().width(40.0).height(40.0), move |r| {
            RenderNode::rect(r, RectStyle::default().with_fill(color_read.get()))
        })
        .unwrap();
        let root = new_container(
            LayoutStyle::new().width(40.0).height(40.0),
            &[canvas.layout_node()],
        )
        .unwrap();
        compute_layout(
            root,
            AvailableSpace::Definite(40.0),
            AvailableSpace::Definite(40.0),
        )
        .unwrap();

        assert_eq!(fill_of(&canvas.view()), Paint::Solid(Color::RED));
        color.set(Color::BLUE);
        assert_eq!(
            fill_of(&canvas.view()),
            Paint::Solid(Color::BLUE),
            "draw closure must be re-read on the second view(), not cached from construction"
        );
    }

    /// Artwork that stands in for a glyph follows the region it is drawn in, the way the text beside it does.
    /// A caret drawn instead of spelled used to be the one mark in a region that had declared its ink that
    /// came out in the theme's.
    #[test]
    fn a_declaring_canvas_paints_with_the_ink_around_it() {
        reset_layout_runtime();
        let canvas = Canvas::declaring(LayoutStyle::new().width(40.0).height(40.0), |r, text| {
            RenderNode::rect(r, RectStyle::default().with_fill(text.color))
        })
        .unwrap();
        let root = new_container(
            LayoutStyle::new().width(40.0).height(40.0),
            &[canvas.layout_node()],
        )
        .unwrap();
        let declared = Color::rgba(0.9, 0.2, 0.1, 1.0);
        crate::declare(
            root,
            renderer_core::Declared::default().with_color(declared),
        );
        compute_layout(
            root,
            AvailableSpace::Definite(40.0),
            AvailableSpace::Definite(40.0),
        )
        .unwrap();

        assert_eq!(fill_of(&canvas.view()), Paint::Solid(declared));
    }

    fn fill_of(view: &RenderNode) -> Paint {
        let RenderNode::Transform { children, .. } = view else {
            panic!("expected Transform")
        };
        let RenderNode::Primitive(DrawCommand::Rect { style, .. }) = &children[0] else {
            panic!("expected a Rect primitive")
        };
        style.fill.expect("expected a fill")
    }
}
