use geometry_core::Rect;
use layout_core::{LayoutError, LayoutStyle, NodeId};
use platform_core::{Event, PointerButton};
use reactive_core::RwSignal;
use ui_tree::{Component, EventResult, RenderNode};

use crate::child_host::{ChildSlot, DynHost};
use crate::context::{new_container, track_layout};
use crate::layout_item::{LayoutItem, TrackedChildren, register_container};
use crate::pointer::dispatch_container_event;
use crate::press::PressGesture;

pub struct Container {
    node: NodeId,
    rect: RwSignal<Rect>,
    // Static children; empty when `dyn_host` is set (a container holding a reactive fragment routes all
    // children — static and dynamic — through the host so they interleave in the layout node).
    children: TrackedChildren,
    dyn_host: Option<DynHost>,
    // Optional tap gesture so a plain row/col can be pressable; children still hit-test first.
    press: PressGesture,
}

impl Container {
    pub fn new(
        layout_style: LayoutStyle,
        children: Vec<Box<dyn LayoutItem>>,
    ) -> Result<Self, LayoutError> {
        let (node, rect, children) = register_container(layout_style, children)?;
        Ok(Container {
            node,
            rect,
            children,
            dyn_host: None,
            press: PressGesture::default(),
        })
    }

    /// A container whose children are a mix of static widgets and reactive fragments (`ChildSlot`s). The
    /// fragments reconcile into this container's own node, so their items are real siblings of the static
    /// children and inherit this container's flex direction/gap — the transparent `for`/`if` path.
    pub fn from_slots(
        layout_style: LayoutStyle,
        slots: Vec<ChildSlot>,
    ) -> Result<Self, LayoutError> {
        let node = new_container(layout_style, &[])?;
        let rect = track_layout(node).expect("new_container always registers a signal");
        let dyn_host = DynHost::build(node, slots)?;
        Ok(Container {
            node,
            rect,
            children: Vec::new(),
            dyn_host: Some(dyn_host),
            press: PressGesture::default(),
        })
    }

    pub fn rect(&self) -> RwSignal<Rect> {
        self.rect.clone()
    }

    fn dispatch_children(&mut self, event: &Event) -> EventResult {
        match &self.dyn_host {
            Some(host) => host.dispatch(event),
            None => dispatch_container_event(&mut self.children, event),
        }
    }

    /// Make the container itself pressable. The callback fires on a tap (release, not press) inside it;
    /// a child widget that handles the press wins, and a scroll gesture started on it does not fire it.
    pub fn on_press(mut self, f: impl Fn() + 'static) -> Self {
        self.press.set(f);
        self
    }

    pub fn column(children: Vec<Box<dyn LayoutItem>>) -> Result<Self, LayoutError> {
        Self::new(LayoutStyle::new().flex_column(), children)
    }
}

impl LayoutItem for Container {
    fn layout_node(&self) -> NodeId {
        self.node
    }
}

impl Component for Container {
    fn view(&self) -> RenderNode {
        // Each child is its own segment: referencing it is a cheap Rc clone, so this view() does not re-run children and is not subscribed to their signals.
        match &self.dyn_host {
            Some(host) => RenderNode::group(host.child_boundaries()),
            None => RenderNode::group(self.children.iter().map(|c| c.segment.boundary())),
        }
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        // No tap handler: behave exactly as before (pure child routing).
        if !self.press.is_set() {
            return self.dispatch_children(event);
        }
        let rect = self.rect.get();
        match event {
            Event::PointerMoved { .. } => {
                self.press.track_move(event);
                self.dispatch_children(event)
            }
            Event::PointerPressed {
                button: PointerButton::Primary,
                ..
            } => {
                if self.dispatch_children(event) == EventResult::Handled {
                    self.press.cancel();
                    return EventResult::Handled;
                }
                self.press.arm(event, rect)
            }
            Event::PointerReleased {
                button: PointerButton::Primary,
                ..
            } => {
                if self.dispatch_children(event) == EventResult::Handled {
                    self.press.cancel();
                    return EventResult::Handled;
                }
                self.press.release(event, rect)
            }
            Event::CursorLeft => {
                self.press.cancel();
                self.dispatch_children(event)
            }
            _ => self.dispatch_children(event),
        }
    }

    fn debug_name(&self) -> &'static str {
        "Container"
    }
}

#[cfg(test)]
mod tests {
    use crate::context::reset_layout_runtime;
    use layout_core::AvailableSpace;
    use platform_core::{Event, PointerSource};
    use renderer_core::{Color, TextStyle};

    use super::*;
    use crate::context::{compute_layout, new_container};
    use crate::text::Text;

    fn make_container_with_labels() -> Container {
        reset_layout_runtime();
        let text_style = TextStyle::new(14.0, Color::WHITE);
        let text_a = Text::new(
            || "A".to_string(),
            LayoutStyle::new().width(50.0).height(20.0),
            move || text_style,
        )
        .unwrap();
        let text_b = Text::new(
            || "B".to_string(),
            LayoutStyle::new().width(50.0).height(20.0),
            move || text_style,
        )
        .unwrap();
        let container = Container::new(
            LayoutStyle::new().flex_row(),
            vec![Box::new(text_a), Box::new(text_b)],
        )
        .unwrap();
        let root = new_container(
            LayoutStyle::new().flex_row().width(200.0).height(100.0),
            &[container.layout_node()],
        )
        .unwrap();
        compute_layout(
            root,
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();
        container
    }

    #[test]
    fn container_row_creates_ok() {
        reset_layout_runtime();
        let result = Container::new(LayoutStyle::new().flex_row(), vec![]);
        assert!(result.is_ok());
    }

    #[test]
    fn container_column_creates_ok() {
        reset_layout_runtime();
        let result = Container::column(vec![]);
        assert!(result.is_ok());
    }

    #[test]
    fn container_view_returns_group_with_children() {
        let container = make_container_with_labels();
        let view = container.view();
        if let RenderNode::Group { children, .. } = view {
            assert_eq!(children.len(), 2);
        } else {
            panic!("expected Group");
        }
    }

    #[test]
    fn container_on_event_returns_ignored_with_no_handlers() {
        let mut container = make_container_with_labels();
        let result = container.on_event(&Event::PointerMoved {
            x: 0.0,
            y: 0.0,
            source: PointerSource::Mouse,
        });
        assert!(matches!(result, EventResult::Ignored));
    }

    #[test]
    fn container_layout_node_is_valid() {
        reset_layout_runtime();
        let container = Container::new(LayoutStyle::new().flex_row(), vec![]).unwrap();
        let node = container.layout_node();
        let _root = new_container(LayoutStyle::new().flex_row(), &[node]).expect("should register");
    }

    #[test]
    fn click_with_force_tick_does_not_panic() {
        use crate::context::track_layout;
        use crate::styled_container::StyledContainer;
        use platform_core::PointerButton;
        use reactive_core::{begin_batch, end_batch, signal};

        reset_layout_runtime();
        let s = signal(0i32);
        let s_cb = s.clone();
        // A pressable primitive stands in for the old high-level Button (now in ui-components).
        let btn = StyledContainer::new(
            LayoutStyle::new().width(50.0).height(30.0),
            |_r| renderer_core::RectStyle::default(),
            vec![],
        )
        .unwrap()
        .on_press(move || s_cb.update(|n| *n += 1));
        let btn_node = btn.layout_node();
        let s_txt = s.clone();
        let txt = crate::text::Text::new(
            move || format!("{}", s_txt.get()),
            LayoutStyle::new().width(50.0).height(20.0),
            || renderer_core::TextStyle::new(14.0, renderer_core::Color::BLACK),
        )
        .unwrap();
        let root = Container::new(
            LayoutStyle::new().flex_column().width(200.0).height(100.0),
            vec![Box::new(btn), Box::new(txt)],
        )
        .unwrap();
        let root_node = root.layout_node();
        compute_layout(
            root_node,
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();
        let br = track_layout(btn_node).unwrap().get();

        let mut tree = crate::ComponentList::new(root);
        let _ = tree.commands();

        // Mimic the runner's event cycle, including the dev-only force-tick. The button fires on release
        // (tap), so send press then release.
        let cx = (br.x + br.width / 2.0) as f64;
        let cy = (br.y + br.height / 2.0) as f64;
        for phase in [true, false] {
            begin_batch();
            let ev = if phase {
                Event::PointerPressed {
                    x: cx,
                    y: cy,
                    button: PointerButton::Primary,
                    source: PointerSource::Mouse,
                }
            } else {
                Event::PointerReleased {
                    x: cx,
                    y: cy,
                    button: PointerButton::Primary,
                    source: PointerSource::Mouse,
                }
            };
            if tree.on_event(&ev) == EventResult::Handled {
                tree.bump_force_ticks();
                end_batch();
                begin_batch();
            }
            let _ = tree.commands();
            end_batch();
        }

        assert_eq!(s.get(), 1, "click should have incremented the signal");
    }

    #[test]
    fn container_can_be_nested_as_layout_item() {
        reset_layout_runtime();
        let inner = Container::column(vec![]).unwrap();
        let outer = Container::new(LayoutStyle::new().flex_row(), vec![Box::new(inner)]);
        assert!(outer.is_ok());
    }
}
