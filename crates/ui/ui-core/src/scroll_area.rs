use platform_core::{Event, ScrollDelta};
use reactive_core::{RwSignal, create_rw_signal};
use renderer_core::{BorderRadius, Color, DrawCommand, Rect, RectStyle};
use ui_tree::{Component, EventResult, View};

use crate::pointer::{dispatch_to_children, offset_pointer, pointer_coords};

pub struct ScrollArea {
    viewport: Box<dyn Fn() -> Rect>,
    content_height: f32,
    scroll_y: RwSignal<f32>,
    children: Vec<Box<dyn Component>>,
}

impl ScrollArea {
    pub fn new(
        viewport: impl Fn() -> Rect + 'static,
        content_height: f32,
        children: Vec<Box<dyn Component>>,
    ) -> Self {
        Self {
            viewport: Box::new(viewport),
            content_height,
            scroll_y: create_rw_signal(0.0),
            children,
        }
    }
}

impl Component for ScrollArea {
    fn view(&self) -> View {
        let vp = (self.viewport)();
        let scroll_y = self.scroll_y.get();

        let content_views: Vec<View> = self.children.iter().map(|c| c.view()).collect();

        let scrollable = View::Clip {
            rect: vp,
            children: vec![View::Translate {
                tx: vp.x,
                ty: vp.y - scroll_y,
                children: content_views,
            }],
        };

        let bar = if self.content_height > vp.height {
            let bar_h = (vp.height / self.content_height * vp.height).max(24.0);
            let max_scroll = (self.content_height - vp.height).max(1.0);
            let bar_y = vp.y + (scroll_y / max_scroll) * (vp.height - bar_h);
            View::Primitive(DrawCommand::Rect {
                rect: Rect::new(vp.x + vp.width - 8.0, bar_y, 6.0, bar_h),
                style: RectStyle::default()
                    .with_fill(Color::rgba(0.5, 0.5, 0.6, 0.6))
                    .with_radius(BorderRadius::all(3.0)),
            })
        } else {
            View::Empty
        };

        View::group([scrollable, bar])
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        let vp = (self.viewport)();

        if let Event::Scrolled { delta } = event {
            let dy = match delta {
                ScrollDelta::Lines { y, .. } => *y * 20.0,
                ScrollDelta::Pixels { y, .. } => *y,
            };
            let max_scroll = (self.content_height - vp.height).max(0.0);
            let new_y = (self.scroll_y.get() - dy).clamp(0.0, max_scroll);
            self.scroll_y.set(new_y);
            return EventResult::Handled;
        }

        if let Some((px, py)) = pointer_coords(event) {
            if !vp.contains(px as f32, py as f32) {
                return EventResult::Ignored;
            }
        }

        let scroll_y = self.scroll_y.get() as f64;
        let adjusted = offset_pointer(event, -(vp.x as f64), -(vp.y as f64) + scroll_y);
        let effective = adjusted.as_ref().unwrap_or(event);
        dispatch_to_children(&mut self.children, effective)
    }
}

#[cfg(test)]
mod tests {
    use platform_core::{PointerSource, ScrollDelta};
    use renderer_core::Rect;

    use super::*;

    fn make_scroll_area() -> ScrollArea {
        ScrollArea::new(|| Rect::new(0.0, 0.0, 400.0, 300.0), 1000.0, vec![])
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
            assert_eq!(children.len(), 2);
            assert!(matches!(&children[0], View::Clip { .. }));
            assert!(matches!(
                &children[1],
                View::Primitive(DrawCommand::Rect { .. })
            ));
        } else {
            panic!("expected Group");
        }
    }

    #[test]
    fn view_no_scrollbar_when_content_fits() {
        let mut sa = make_scroll_area();
        sa.content_height = 200.0;
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

        struct CapturingComponent {
            out: Rc<Cell<f64>>,
        }
        impl Component for CapturingComponent {
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

        let mut sa = ScrollArea {
            viewport: Box::new(|| Rect::new(100.0, 50.0, 400.0, 300.0)),
            content_height: 1000.0,
            scroll_y: create_rw_signal(100.0),
            children: vec![Box::new(CapturingComponent {
                out: captured_y_clone,
            })],
        };

        sa.on_event(&Event::PointerMoved {
            x: 150.0,
            y: 200.0,
            source: PointerSource::Mouse,
        });

        assert!((captured_y.get() - 250.0).abs() < 0.001);
    }
}
