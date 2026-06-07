use geometry_core::Rect;
use layout_core::{LayoutError, LayoutStyle};
use platform_core::{Event, ScrollDelta};
use reactive_core::{RwSignal, create_rw_signal};
use renderer_core::{BorderRadius, Color, DrawCommand, RectPayload, RectStyle};
use ui_tree::{Component, EventResult, RenderNode};

use ui_tree::NodeVec;

use crate::context::{WidgetCtx, track_layout};
use crate::layout_item::{LayoutItem, LeafWidget};
use crate::layout_leaf::LayoutLeaf;
use crate::pointer::{clip_pointer_event, offset_pointer};

pub struct ScrollbarStyle {
    pub color: Color,
    pub width: f32,
    pub radius: f32,
}

impl Default for ScrollbarStyle {
    fn default() -> Self {
        Self {
            color: Color::rgba(0.5, 0.5, 0.6, 0.6),
            width: 8.0,
            radius: 3.0,
        }
    }
}

pub struct ScrollArea {
    viewport: Box<dyn Fn() -> Rect>,
    content_size: RwSignal<Rect>,
    // Reactive model: signal writes automatically trigger view() re-evaluation.
    scroll_x: RwSignal<f32>,
    // Reactive model: signal writes automatically trigger view() re-evaluation.
    scroll_y: RwSignal<f32>,
    content: Box<dyn LayoutItem>,
    scrollbar_style: ScrollbarStyle,
    // When Some, the ScrollArea is a LayoutItem and its viewport is taken from this leaf's taffy-computed rect instead of the `viewport` closure.
    layout_leaf: Option<LayoutLeaf>,
}

impl ScrollArea {
    pub fn new(
        ctx: &WidgetCtx,
        viewport: impl Fn() -> Rect + 'static,
        content: Box<dyn LayoutItem>,
    ) -> Self {
        let content_size =
            track_layout(ctx, content.layout_node()).expect("content node not registered in ctx");
        Self {
            viewport: Box::new(viewport),
            content_size,
            scroll_x: create_rw_signal(0.0),
            scroll_y: create_rw_signal(0.0),
            content,
            scrollbar_style: ScrollbarStyle::default(),
            layout_leaf: None,
        }
    }

    /// Creates a ScrollArea that acts as a LayoutItem (can be a child of Container).
    /// The viewport size is determined by the layout system (taffy), not a closure.
    pub fn as_layout_item(
        ctx: &mut WidgetCtx,
        layout: LayoutStyle,
        content: Box<dyn LayoutItem>,
    ) -> Result<Self, LayoutError> {
        let content_size =
            track_layout(ctx, content.layout_node()).expect("content node not registered in ctx");
        let leaf = LayoutLeaf::register(ctx, layout)?;
        Ok(Self {
            // Unused when `layout_leaf` is Some; the viewport comes from the leaf's taffy rect.
            viewport: Box::new(Rect::default),
            content_size,
            scroll_x: create_rw_signal(0.0),
            scroll_y: create_rw_signal(0.0),
            content,
            scrollbar_style: ScrollbarStyle::default(),
            layout_leaf: Some(leaf),
        })
    }

    fn viewport_rect(&self) -> Rect {
        match &self.layout_leaf {
            Some(leaf) => leaf.rect.get(),
            None => (self.viewport)(),
        }
    }

    pub fn scrollbar_style(mut self, style: ScrollbarStyle) -> Self {
        self.scrollbar_style = style;
        self
    }

    pub fn clamp_scroll(&mut self) {
        let vp = self.viewport_rect();
        let content_rect = self.content_size.get();
        let max_x = (content_rect.width - vp.width).max(0.0);
        let max_y = (content_rect.height - vp.height).max(0.0);
        let cx = self.scroll_x.get().clamp(0.0, max_x);
        let cy = self.scroll_y.get().clamp(0.0, max_y);
        if self.scroll_x.get() != cx {
            self.scroll_x.set(cx);
        }
        if self.scroll_y.get() != cy {
            self.scroll_y.set(cy);
        }
    }
}

impl Component for ScrollArea {
    fn view(&self) -> RenderNode {
        let vp = self.viewport_rect();
        let scroll_x = self.scroll_x.get();
        let scroll_y = self.scroll_y.get();
        let content_rect = self.content_size.get();

        let scrollable = RenderNode::Clip {
            rect: vp,
            radius: BorderRadius::zero(),
            children: NodeVec::collect([RenderNode::Transform {
                matrix: [1.0, 0.0, 0.0, 1.0, vp.x - scroll_x, vp.y - scroll_y],
                children: NodeVec::collect([self.content.view()]),
            }]),
        };

        let sb = &self.scrollbar_style;
        let vbar = if content_rect.height > vp.height {
            let bar_h = (vp.height / content_rect.height * vp.height).max(24.0);
            let max_scroll = (content_rect.height - vp.height).max(1.0);
            let bar_y = vp.y + (scroll_y / max_scroll) * (vp.height - bar_h);
            RenderNode::Primitive(DrawCommand::Rect(Box::new(RectPayload {
                rect: Rect::new(vp.x + vp.width - sb.width, bar_y, sb.width - 2.0, bar_h),
                style: RectStyle::default()
                    .with_fill(sb.color)
                    .with_radius(BorderRadius::all(sb.radius)),
            })))
        } else {
            RenderNode::Empty
        };

        let hbar = if content_rect.width > vp.width {
            let bar_w = (vp.width / content_rect.width * vp.width).max(24.0);
            let max_scroll_x = (content_rect.width - vp.width).max(1.0);
            let bar_x = vp.x + (scroll_x / max_scroll_x) * (vp.width - bar_w);
            RenderNode::Primitive(DrawCommand::Rect(Box::new(RectPayload {
                rect: Rect::new(bar_x, vp.y + vp.height - sb.width, bar_w, sb.width - 2.0),
                style: RectStyle::default()
                    .with_fill(sb.color)
                    .with_radius(BorderRadius::all(sb.radius)),
            })))
        } else {
            RenderNode::Empty
        };

        RenderNode::group([scrollable, vbar, hbar])
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        let vp = self.viewport_rect();

        if let Event::Scrolled { delta } = event {
            let (dx, dy) = match delta {
                ScrollDelta::Lines { x, y } => (*x * 20.0, *y * 20.0),
                ScrollDelta::Pixels { x, y } => (*x, *y),
            };
            let content_rect = self.content_size.get();
            let max_scroll_x = (content_rect.width - vp.width).max(0.0);
            let max_scroll_y = (content_rect.height - vp.height).max(0.0);
            self.scroll_x
                .set((self.scroll_x.get() - dx).clamp(0.0, max_scroll_x));
            self.scroll_y
                .set((self.scroll_y.get() - dy).clamp(0.0, max_scroll_y));
            return EventResult::Handled;
        }

        let Some(event) = clip_pointer_event(event, vp) else {
            return EventResult::Ignored;
        };

        let scroll_x = self.scroll_x.get() as f64;
        let scroll_y = self.scroll_y.get() as f64;
        let adjusted = offset_pointer(event, vp.x as f64 - scroll_x, vp.y as f64 - scroll_y);
        let effective = adjusted.as_ref().unwrap_or(event);
        self.content.on_event(effective)
    }
}

impl LeafWidget for ScrollArea {
    fn layout_leaf(&self) -> &LayoutLeaf {
        self.layout_leaf
            .as_ref()
            .expect("ScrollArea must be created with as_layout_item to use as LayoutItem")
    }
}

#[cfg(test)]
mod tests {
    use geometry_core::Rect;
    use layout_core::{AvailableSpace, LayoutStyle, NodeId};
    use platform_core::{Event, PointerSource, ScrollDelta};
    use ui_tree::{Component, EventResult, RenderNode};

    use super::*;
    use crate::context::{WidgetCtx, compute_layout, new_container, with_context};
    use crate::drawing_area::Canvas;
    use crate::layout_item::LayoutItem;
    use crate::layout_leaf::LayoutLeaf;

    fn make_scroll_area() -> ScrollArea {
        let (sa, _ctx) = with_context(WidgetCtx::new(), |ctx| {
            let content = Canvas::new(ctx, LayoutStyle::new().width(400.0).height(1000.0), |_| {
                RenderNode::Empty
            })
            .unwrap();
            let node = content.layout_node();
            let sa = ScrollArea::new(ctx, || Rect::new(0.0, 0.0, 400.0, 300.0), Box::new(content));
            compute_layout(
                ctx,
                node,
                AvailableSpace::Definite(400.0),
                AvailableSpace::MaxContent,
            )
            .unwrap();
            sa
        });
        sa
    }

    fn make_scroll_area_small() -> ScrollArea {
        let (sa, _ctx) = with_context(WidgetCtx::new(), |ctx| {
            let content = Canvas::new(ctx, LayoutStyle::new().width(400.0).height(200.0), |_| {
                RenderNode::Empty
            })
            .unwrap();
            let node = content.layout_node();
            let sa = ScrollArea::new(ctx, || Rect::new(0.0, 0.0, 400.0, 300.0), Box::new(content));
            compute_layout(
                ctx,
                node,
                AvailableSpace::Definite(400.0),
                AvailableSpace::MaxContent,
            )
            .unwrap();
            sa
        });
        sa
    }

    #[test]
    fn as_layout_item_uses_leaf_rect_as_viewport() {
        let (sa, _ctx) = with_context(WidgetCtx::new(), |ctx| {
            let content = Canvas::new(ctx, LayoutStyle::new().width(400.0).height(1000.0), |_| {
                RenderNode::Empty
            })
            .unwrap();
            let content_node = content.layout_node();
            let sa = ScrollArea::as_layout_item(
                ctx,
                LayoutStyle::new().width(400.0).height(300.0),
                Box::new(content),
            )
            .unwrap();
            let root = new_container(
                ctx,
                LayoutStyle::new().flex_column().width(400.0).height(300.0),
                &[sa.layout_node()],
            )
            .unwrap();
            compute_layout(
                ctx,
                root,
                AvailableSpace::Definite(400.0),
                AvailableSpace::Definite(300.0),
            )
            .unwrap();
            compute_layout(
                ctx,
                content_node,
                AvailableSpace::Definite(400.0),
                AvailableSpace::MaxContent,
            )
            .unwrap();
            sa
        });
        let vp = sa.viewport_rect();
        assert_eq!(vp.width, 400.0);
        assert_eq!(vp.height, 300.0);
    }

    #[test]
    fn as_layout_item_emits_clip_and_vbar_on_overflow() {
        let (sa, _ctx) = with_context(WidgetCtx::new(), |ctx| {
            let content = Canvas::new(ctx, LayoutStyle::new().width(400.0).height(1000.0), |_| {
                RenderNode::Empty
            })
            .unwrap();
            let content_node = content.layout_node();
            let sa = ScrollArea::as_layout_item(
                ctx,
                LayoutStyle::new().width(400.0).height(300.0),
                Box::new(content),
            )
            .unwrap();
            let root = new_container(
                ctx,
                LayoutStyle::new().flex_column().width(400.0).height(300.0),
                &[sa.layout_node()],
            )
            .unwrap();
            compute_layout(
                ctx,
                root,
                AvailableSpace::Definite(400.0),
                AvailableSpace::Definite(300.0),
            )
            .unwrap();
            compute_layout(
                ctx,
                content_node,
                AvailableSpace::Definite(400.0),
                AvailableSpace::MaxContent,
            )
            .unwrap();
            sa
        });
        if let RenderNode::Group(children) = sa.view() {
            assert_eq!(children.len(), 3);
            assert!(matches!(&children[0], RenderNode::Clip { .. }));
            assert!(matches!(
                &children[1],
                RenderNode::Primitive(DrawCommand::Rect(_))
            ));
        } else {
            panic!("expected Group");
        }
    }

    #[test]
    fn scroll_lines_updates_offset() {
        let mut sa = make_scroll_area();
        sa.on_event(&Event::Scrolled {
            delta: ScrollDelta::Lines { x: 0.0, y: -3.0 },
        });
        assert_eq!(sa.scroll_y.get(), 60.0);
    }

    #[test]
    fn scroll_pixels_updates_offset() {
        let mut sa = make_scroll_area();
        sa.on_event(&Event::Scrolled {
            delta: ScrollDelta::Pixels { x: 0.0, y: -80.0 },
        });
        assert_eq!(sa.scroll_y.get(), 80.0);
    }

    #[test]
    fn scroll_clamps_to_max() {
        let mut sa = make_scroll_area();
        sa.on_event(&Event::Scrolled {
            delta: ScrollDelta::Pixels { x: 0.0, y: -9999.0 },
        });
        assert_eq!(sa.scroll_y.get(), 700.0);
    }

    #[test]
    fn scroll_clamps_to_zero() {
        let mut sa = make_scroll_area();
        sa.on_event(&Event::Scrolled {
            delta: ScrollDelta::Pixels { x: 0.0, y: 9999.0 },
        });
        assert_eq!(sa.scroll_y.get(), 0.0);
    }

    #[test]
    fn pointer_outside_viewport_is_ignored() {
        let mut sa = make_scroll_area();
        let result = sa.on_event(&Event::PointerMoved {
            x: 500.0,
            y: 100.0,
            source: PointerSource::Mouse,
        });
        assert!(matches!(result, EventResult::Ignored));
    }

    #[test]
    fn view_emits_clip_and_scrollbar_when_content_overflows() {
        let sa = make_scroll_area();
        let view = sa.view();
        if let RenderNode::Group(children) = view {
            assert_eq!(children.len(), 3);
            assert!(matches!(&children[0], RenderNode::Clip { .. }));
            assert!(matches!(
                &children[1],
                RenderNode::Primitive(DrawCommand::Rect(_))
            ));
        } else {
            panic!("expected Group");
        }
    }

    #[test]
    fn view_no_scrollbar_when_content_fits() {
        let sa = make_scroll_area_small();
        let view = sa.view();
        if let RenderNode::Group(children) = view {
            assert!(matches!(&children[1], RenderNode::Empty));
        } else {
            panic!("expected Group");
        }
    }

    #[test]
    fn child_receives_offset_pointer_event() {
        use std::cell::Cell;
        use std::rc::Rc;

        let captured_y: Rc<Cell<f64>> = Rc::new(Cell::new(-1.0));
        let captured_y_clone = captured_y.clone();

        struct CapturingItem {
            leaf: LayoutLeaf,
            out: Rc<Cell<f64>>,
        }
        impl Component for CapturingItem {
            fn view(&self) -> RenderNode {
                RenderNode::Empty
            }
            fn on_event(&mut self, event: &Event) -> EventResult {
                if let Event::PointerMoved { y, .. } = event {
                    self.out.set(*y);
                    EventResult::Handled
                } else {
                    EventResult::Ignored
                }
            }
        }
        impl LayoutItem for CapturingItem {
            fn layout_node(&self) -> NodeId {
                self.leaf.node
            }
        }

        let (mut sa, _ctx) = with_context(WidgetCtx::new(), |ctx| {
            let leaf =
                LayoutLeaf::register(ctx, LayoutStyle::new().width(400.0).height(1000.0)).unwrap();
            let node = leaf.node;
            let content = CapturingItem {
                leaf,
                out: captured_y_clone,
            };
            let sa = ScrollArea::new(
                ctx,
                || Rect::new(100.0, 50.0, 400.0, 300.0),
                Box::new(content),
            );
            compute_layout(
                ctx,
                node,
                AvailableSpace::Definite(400.0),
                AvailableSpace::MaxContent,
            )
            .unwrap();
            sa
        });

        // Set scroll_y to 100 via a scroll event
        sa.on_event(&Event::Scrolled {
            delta: ScrollDelta::Pixels { x: 0.0, y: -100.0 },
        });

        sa.on_event(&Event::PointerMoved {
            x: 150.0,
            y: 200.0,
            source: PointerSource::Mouse,
        });

        assert!((captured_y.get() - 250.0).abs() < 0.001);
    }

    fn make_scroll_area_wide() -> ScrollArea {
        let (sa, _ctx) = with_context(WidgetCtx::new(), |ctx| {
            let content = Canvas::new(ctx, LayoutStyle::new().width(1000.0).height(300.0), |_| {
                RenderNode::Empty
            })
            .unwrap();
            let node = content.layout_node();
            let sa = ScrollArea::new(ctx, || Rect::new(0.0, 0.0, 400.0, 300.0), Box::new(content));
            compute_layout(
                ctx,
                node,
                AvailableSpace::Definite(1000.0),
                AvailableSpace::MaxContent,
            )
            .unwrap();
            sa
        });
        sa
    }

    #[test]
    fn scroll_x_lines_updates_offset() {
        let mut sa = make_scroll_area_wide();
        sa.on_event(&Event::Scrolled {
            delta: ScrollDelta::Lines { x: -3.0, y: 0.0 },
        });
        assert_eq!(sa.scroll_x.get(), 60.0);
    }

    #[test]
    fn scroll_x_pixels_updates_offset() {
        let mut sa = make_scroll_area_wide();
        sa.on_event(&Event::Scrolled {
            delta: ScrollDelta::Pixels { x: -80.0, y: 0.0 },
        });
        assert_eq!(sa.scroll_x.get(), 80.0);
    }

    #[test]
    fn scroll_x_clamps_to_max() {
        let mut sa = make_scroll_area_wide();
        sa.on_event(&Event::Scrolled {
            delta: ScrollDelta::Pixels { x: -9999.0, y: 0.0 },
        });
        assert_eq!(sa.scroll_x.get(), 600.0);
    }

    #[test]
    fn scroll_x_clamps_to_zero() {
        let mut sa = make_scroll_area_wide();
        sa.on_event(&Event::Scrolled {
            delta: ScrollDelta::Pixels { x: 9999.0, y: 0.0 },
        });
        assert_eq!(sa.scroll_x.get(), 0.0);
    }

    #[test]
    fn view_emits_hbar_when_content_overflows_x() {
        let sa = make_scroll_area_wide();
        let view = sa.view();
        if let RenderNode::Group(children) = view {
            assert_eq!(children.len(), 3);
            assert!(matches!(&children[0], RenderNode::Clip { .. }));
            assert!(matches!(&children[1], RenderNode::Empty));
            assert!(matches!(
                &children[2],
                RenderNode::Primitive(DrawCommand::Rect(_))
            ));
        } else {
            panic!("expected Group");
        }
    }

    #[test]
    fn view_no_hbar_when_content_fits_x() {
        let sa = make_scroll_area();
        let view = sa.view();
        if let RenderNode::Group(children) = view {
            assert_eq!(children.len(), 3);
            assert!(matches!(&children[2], RenderNode::Empty));
        } else {
            panic!("expected Group");
        }
    }
}
