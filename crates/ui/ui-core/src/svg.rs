use std::sync::Arc;

use layout_core::{LayoutError, LayoutStyle};
use platform_core::Event;
use renderer_core::{Color, SvgData};
use ui_tree::{Component, EventResult, NodeVec, RenderNode};

use crate::impl_leaf_widget;
use crate::layout_leaf::LayoutLeaf;

pub struct Svg {
    data: Box<dyn Fn() -> Arc<SvgData>>,
    tint: Box<dyn Fn() -> Option<Color>>,
    leaf: LayoutLeaf,
}

impl Svg {
    pub fn new(
        ctx: &mut crate::context::WidgetCtx,
        layout_style: LayoutStyle,
        data_fn: impl Fn() -> Arc<SvgData> + 'static,
        tint_fn: impl Fn() -> Option<Color> + 'static,
    ) -> Result<Self, LayoutError> {
        // Parity with `<img>`: a side left at `auto` falls back to the SVG's intrinsic size, and a single px side derives the other from the intrinsic aspect ratio; a percent side is left untouched.
        let width_auto = layout_style.is_width_auto();
        let height_auto = layout_style.is_height_auto();
        let layout_style = if width_auto && height_auto {
            let (iw, ih) = data_fn().intrinsic_size();
            layout_style.width(iw).height(ih)
        } else if width_auto {
            match layout_style.height_px() {
                Some(h) => {
                    let (iw, ih) = data_fn().intrinsic_size();
                    if ih > 0.0 {
                        layout_style.width(h * iw / ih)
                    } else {
                        layout_style
                    }
                }
                None => layout_style,
            }
        } else if height_auto {
            match layout_style.width_px() {
                Some(w) => {
                    let (iw, ih) = data_fn().intrinsic_size();
                    if iw > 0.0 {
                        layout_style.height(w * ih / iw)
                    } else {
                        layout_style
                    }
                }
                None => layout_style,
            }
        } else {
            layout_style
        };

        let leaf = LayoutLeaf::register(ctx, layout_style)?;
        Ok(Self {
            data: Box::new(data_fn),
            tint: Box::new(tint_fn),
            leaf,
        })
    }
}

impl Component for Svg {
    fn view(&self) -> RenderNode {
        let r = self.leaf.rect.get();
        let commands = (self.data)().commands_for(r.width, r.height, (self.tint)());
        let children = NodeVec::collect(commands.iter().cloned().map(RenderNode::Primitive));
        self.leaf.at_layout_position(RenderNode::Group { children })
    }

    fn on_event(&mut self, _event: &Event) -> EventResult {
        EventResult::Ignored
    }

    fn debug_name(&self) -> &'static str {
        "Svg"
    }
}

impl_leaf_widget!(Svg);

#[cfg(test)]
mod tests {
    use layout_core::AvailableSpace;
    use renderer_core::DrawCommand;

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
}
