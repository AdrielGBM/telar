use std::sync::Arc;

use geometry_core::{ObjectFit, Rect};
use layout_core::{LayoutError, LayoutStyle};
use platform_core::Event;
use renderer_assets::SvgData;
use renderer_core::{BorderRadius, Color};
use ui_tree::{Component, EventResult, NodeVec, RenderNode};

use crate::impl_leaf_widget;
use crate::layout_leaf::LayoutLeaf;

pub struct Svg {
    data: Box<dyn Fn() -> Arc<SvgData>>,
    tint: Box<dyn Fn() -> Option<Color>>,
    fit: Box<dyn Fn() -> ObjectFit>,
    leaf: LayoutLeaf,
}

impl Svg {
    pub fn new(
        ctx: &mut crate::context::WidgetCtx,
        layout_style: LayoutStyle,
        data_fn: impl Fn() -> Arc<SvgData> + 'static,
        tint_fn: impl Fn() -> Option<Color> + 'static,
        fit_fn: impl Fn() -> ObjectFit + 'static,
    ) -> Result<Self, LayoutError> {
        // A side left at `auto` falls back to the SVG's intrinsic size; a single px side derives the other from the intrinsic aspect ratio; a percent side is left untouched.
        let layout_style =
            crate::layout_leaf::resolve_intrinsic_size(layout_style, || data_fn().intrinsic_size());

        let leaf = LayoutLeaf::register(ctx, layout_style)?;
        Ok(Self {
            data: Box::new(data_fn),
            tint: Box::new(tint_fn),
            fit: Box::new(fit_fn),
            leaf,
        })
    }
}

impl Component for Svg {
    fn view(&self) -> RenderNode {
        let r = self.leaf.rect.get();
        let fit = (self.fit)();
        let commands = (self.data)().commands_for(r.width, r.height, (self.tint)(), fit);
        let children = NodeVec::collect(commands.iter().cloned().map(RenderNode::Primitive));
        let group = RenderNode::Group { children };
        // Cover scales the paths past the box; clip the overflow to the widget's local box. The renderer maps clip rects through the active matrix, so a local (0,0,w,h) clip composes with this widget's layout transform and any scroll.
        let node = if fit == ObjectFit::Cover {
            RenderNode::Clip {
                rect: Rect {
                    x: 0.0,
                    y: 0.0,
                    width: r.width,
                    height: r.height,
                },
                radius: BorderRadius::zero(),
                children: NodeVec::collect([group]),
            }
        } else {
            group
        };
        self.leaf.at_layout_position(node)
    }

    fn on_event(&mut self, _event: &Event) -> EventResult {
        EventResult::Ignored
    }

    fn debug_name(&self) -> &'static str {
        "Svg"
    }
}

impl_leaf_widget!(Svg);

// Gated on `dynamic-svg`: these tests build `SvgData` with `from_str`, which is unavailable under bare `svg`.
#[cfg(all(test, feature = "dynamic-svg"))]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use layout_core::AvailableSpace;
    use renderer_core::{DrawCommand, Paint};

    use super::*;
    use crate::context::{WidgetCtx, compute_layout, new_container};
    use crate::layout_item::LayoutItem;

    fn make_svg_data(w: f32, h: f32) -> Arc<SvgData> {
        let src = format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}"><rect width="{w}" height="{h}" fill="#ff0000"/></svg>"##
        );
        Arc::new(SvgData::from_str(&src).unwrap())
    }

    #[test]
    fn svg_without_size_uses_intrinsic_size() {
        let mut ctx = WidgetCtx::new();
        let data = make_svg_data(24.0, 16.0);
        let svg = Svg::new(
            &mut ctx,
            LayoutStyle::new(),
            move || Arc::clone(&data),
            || None,
            || ObjectFit::Contain,
        )
        .unwrap();
        let root = new_container(
            &mut ctx,
            LayoutStyle::new().flex_column().width(200.0).height(200.0),
            &[svg.layout_node()],
        )
        .unwrap();
        compute_layout(
            &mut ctx,
            root,
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(200.0),
        )
        .unwrap();

        let rect = svg.leaf.rect.get();
        assert_eq!(rect.width, 24.0);
        assert_eq!(rect.height, 16.0);
    }

    #[test]
    fn svg_with_only_width_derives_height_from_aspect_ratio() {
        let mut ctx = WidgetCtx::new();
        let data = make_svg_data(24.0, 16.0); // 1.5:1 aspect ratio
        let svg = Svg::new(
            &mut ctx,
            LayoutStyle::new().width(48.0),
            move || Arc::clone(&data),
            || None,
            || ObjectFit::Contain,
        )
        .unwrap();
        let root = new_container(
            &mut ctx,
            LayoutStyle::new().flex_column().width(200.0).height(200.0),
            &[svg.layout_node()],
        )
        .unwrap();
        compute_layout(
            &mut ctx,
            root,
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(200.0),
        )
        .unwrap();

        let rect = svg.leaf.rect.get();
        assert_eq!(rect.width, 48.0);
        assert_eq!(rect.height, 32.0);
    }

    #[test]
    fn svg_view_emits_group_with_path_primitive() {
        let mut ctx = WidgetCtx::new();
        let data = make_svg_data(10.0, 10.0);
        let svg = Svg::new(
            &mut ctx,
            LayoutStyle::new().width(20.0).height(20.0),
            move || Arc::clone(&data),
            || None,
            || ObjectFit::Contain,
        )
        .unwrap();
        let root = new_container(
            &mut ctx,
            LayoutStyle::new().width(20.0).height(20.0),
            &[svg.layout_node()],
        )
        .unwrap();
        compute_layout(
            &mut ctx,
            root,
            AvailableSpace::Definite(20.0),
            AvailableSpace::Definite(20.0),
        )
        .unwrap();

        let view = svg.view();
        let RenderNode::Transform { children, .. } = view else {
            panic!("expected Transform")
        };
        assert_eq!(children.len(), 1);
        let RenderNode::Group { children: inner } = &children[0] else {
            panic!("expected Group inside Transform")
        };
        assert!(
            inner
                .iter()
                .any(|n| matches!(n, RenderNode::Primitive(DrawCommand::Path { .. }))),
            "expected at least one Path primitive among {} children",
            inner.len()
        );
    }

    // `tint_fn` must be invoked fresh on every `view()` (not cached from construction), so a `$signal`
    // tint (the reactive path the transpiler now wires for `svg tint:$accent`) recolors live.
    #[test]
    fn tint_closure_is_re_read_each_view_and_recolors_the_path() {
        let mut ctx = WidgetCtx::new();
        let data = make_svg_data(10.0, 10.0);
        let tint = Rc::new(Cell::new(Color::GREEN));
        let tint_read = tint.clone();
        let svg = Svg::new(
            &mut ctx,
            LayoutStyle::new().width(20.0).height(20.0),
            move || Arc::clone(&data),
            move || Some(tint_read.get()),
            || ObjectFit::Contain,
        )
        .unwrap();
        let root = new_container(
            &mut ctx,
            LayoutStyle::new().width(20.0).height(20.0),
            &[svg.layout_node()],
        )
        .unwrap();
        compute_layout(
            &mut ctx,
            root,
            AvailableSpace::Definite(20.0),
            AvailableSpace::Definite(20.0),
        )
        .unwrap();

        assert_eq!(path_fill(&svg.view()), Paint::Solid(Color::GREEN));
        tint.set(Color::BLUE);
        assert_eq!(
            path_fill(&svg.view()),
            Paint::Solid(Color::BLUE),
            "tint closure must be re-read on the second view(), not cached from construction"
        );
    }

    fn path_fill(view: &RenderNode) -> Paint {
        let RenderNode::Transform { children, .. } = view else {
            panic!("expected Transform")
        };
        let RenderNode::Group { children: inner } = &children[0] else {
            panic!("expected Group inside Transform")
        };
        inner
            .iter()
            .find_map(|n| match n {
                RenderNode::Primitive(DrawCommand::Path { style, .. }) => style.fill,
                _ => None,
            })
            .expect("expected a filled Path primitive")
    }
}
