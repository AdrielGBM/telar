use geometry_core::Rect;
use platform_core::{Event, ScrollDelta};
use reactive_core::{RwSignal, create_rw_signal};
use renderer_core::{BorderRadius, Color, DrawCommand, RectPayload, RectStyle};
use ui_tree::{Component, EventResult, View};

use crate::context::{WidgetCtx, track_layout};
use crate::layout_item::LayoutItem;
use crate::pointer::{offset_pointer, pointer_coords};

pub struct ScrollArea {
    viewport: Box<dyn Fn() -> Rect>,
    content_size: RwSignal<Rect>,
    scroll_x: RwSignal<f32>,
    scroll_y: RwSignal<f32>,
    content: Box<dyn LayoutItem>,
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
        }
    }

    pub fn clamp_scroll(&mut self) {
        let vp = (self.viewport)();
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
    fn view(&self) -> View {
        let vp = (self.viewport)();
        let scroll_x = self.scroll_x.get();
        let scroll_y = self.scroll_y.get();
        let content_rect = self.content_size.get();

        let scrollable = View::Clip {
            rect: vp,
            children: vec![View::Transform {
                matrix: [1.0, 0.0, 0.0, 1.0, vp.x - scroll_x, vp.y - scroll_y],
                children: vec![self.content.view()],
            }],
        };

        let vbar = if content_rect.height > vp.height {
            let bar_h = (vp.height / content_rect.height * vp.height).max(24.0);
            let max_scroll = (content_rect.height - vp.height).max(1.0);
            let bar_y = vp.y + (scroll_y / max_scroll) * (vp.height - bar_h);
            View::Primitive(DrawCommand::Rect(Box::new(RectPayload {
                rect: Rect::new(vp.x + vp.width - 8.0, bar_y, 6.0, bar_h),
                style: RectStyle::default()
                    .with_fill(Color::rgba(0.5, 0.5, 0.6, 0.6))
                    .with_radius(BorderRadius::all(3.0)),
            })))
        } else {
            View::Empty
        };

        let hbar = if content_rect.width > vp.width {
            let bar_w = (vp.width / content_rect.width * vp.width).max(24.0);
            let max_scroll_x = (content_rect.width - vp.width).max(1.0);
            let bar_x = vp.x + (scroll_x / max_scroll_x) * (vp.width - bar_w);
            View::Primitive(DrawCommand::Rect(Box::new(RectPayload {
                rect: Rect::new(bar_x, vp.y + vp.height - 8.0, bar_w, 6.0),
                style: RectStyle::default()
                    .with_fill(Color::rgba(0.5, 0.5, 0.6, 0.6))
                    .with_radius(BorderRadius::all(3.0)),
            })))
        } else {
            View::Empty
        };

        View::group([scrollable, vbar, hbar])
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        let vp = (self.viewport)();

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

        if let Some((px, py)) = pointer_coords(event) {
            if !vp.contains(px as f32, py as f32) {
                return EventResult::Ignored;
            }
        }

        let scroll_x = self.scroll_x.get() as f64;
        let scroll_y = self.scroll_y.get() as f64;
        let adjusted = offset_pointer(event, -(vp.x as f64) + scroll_x, -(vp.y as f64) + scroll_y);
        let effective = adjusted.as_ref().unwrap_or(event);
        self.content.on_event(effective)
    }
}

#[cfg(test)]
mod tests {
    use geometry_core::Rect;
    use layout_core::{AvailableSpace, LayoutStyle, NodeId};
    use platform_core::{Event, PointerSource, ScrollDelta};
    use ui_tree::{Component, EventResult, View};

    use super::*;
    use crate::context::{WidgetCtx, compute_layout, with_context};
    use crate::drawing_area::DrawingArea;
    use crate::layout_item::LayoutItem;
    use crate::layout_leaf::LayoutLeaf;

    fn make_scroll_area() -> ScrollArea {
        let (sa, _ctx) = with_context(WidgetCtx::new(), |ctx| {
            let content = DrawingArea::new(
                ctx,
                LayoutStyle::new().width(400.0).height(1000.0),
                |_, _| View::Empty,
            )
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
            let content = DrawingArea::new(
                ctx,
                LayoutStyle::new().width(400.0).height(200.0),
                |_, _| View::Empty,
            )
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
        if let View::Group(children) = view {
            assert_eq!(children.len(), 3);
            assert!(matches!(&children[0], View::Clip { .. }));
            assert!(matches!(
                &children[1],
                View::Primitive(DrawCommand::Rect(_))
            ));
        } else {
            panic!("expected Group");
        }
    }

    #[test]
    fn view_no_scrollbar_when_content_fits() {
        let sa = make_scroll_area_small();
        let view = sa.view();
        if let View::Group(children) = view {
            assert!(matches!(&children[1], View::Empty));
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
            fn view(&self) -> View {
                View::Empty
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
            let content = DrawingArea::new(
                ctx,
                LayoutStyle::new().width(1000.0).height(300.0),
                |_, _| View::Empty,
            )
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
        if let View::Group(children) = view {
            assert_eq!(children.len(), 3);
            assert!(matches!(&children[0], View::Clip { .. }));
            assert!(matches!(&children[1], View::Empty));
            assert!(matches!(
                &children[2],
                View::Primitive(DrawCommand::Rect(_))
            ));
        } else {
            panic!("expected Group");
        }
    }

    #[test]
    fn view_no_hbar_when_content_fits_x() {
        let sa = make_scroll_area();
        let view = sa.view();
        if let View::Group(children) = view {
            assert_eq!(children.len(), 3);
            assert!(matches!(&children[2], View::Empty));
        } else {
            panic!("expected Group");
        }
    }
}
