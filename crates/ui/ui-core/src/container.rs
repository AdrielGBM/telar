use geometry_core::Rect;
use layout_core::{LayoutError, LayoutStyle, NodeId};
use platform_core::{Event, PointerButton};
use reactive_core::RwSignal;
use renderer_core::Declared;
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

    fn dispatch_children(&mut self, event: &Event) -> EventResult {
        match &self.dyn_host {
            Some(host) => host.dispatch(event),
            None => dispatch_container_event(&mut self.children, event),
        }
    }

    /// Says what the text below this container looks like — everything under it, not the container itself,
    /// which draws no text at all.
    ///
    /// Re-run when a signal the declaration read changes, and withdrawn when this container goes, so a
    /// subtree that is rebuilt does not inherit from the one it replaced.
    pub fn declaring(self, declared: impl Fn() -> Declared + 'static) -> Self {
        let node = self.node;
        reactive_core::effect(move || crate::inherit::declare(node, declared()));
        self
    }

    /// Keeps this container's layout style in step with the reactive state it was built from — see
    /// [`StyledContainer::styled_by`](crate::StyledContainer::styled_by), which is the same thing on a box that
    /// also paints.
    pub fn styled_by(self, style: impl Fn() -> LayoutStyle + 'static) -> Self {
        let node = self.node;
        crate::styled_container::style_follows(node, style);
        self
    }

    /// Make the container itself pressable. The callback fires on a tap (release, not press) inside it;
    /// a child widget that handles the press wins, and a scroll gesture started on it does not fire it.
    pub fn on_press(self, f: impl Fn() + 'static) -> Self {
        self.maybe_on_press(Some(f))
    }

    /// [`on_press`](Self::on_press) for a handler the caller may not have supplied.
    ///
    /// The emitter picks this form for any `on_press:` whose value is not a closure literal, which is how a
    /// wrapper component forwards an `Option` — and a `Container` reached that emitter with no such method,
    /// so a plain container forwarding one did not compile. `None` leaves the container untouched: a no-op
    /// handler would still report the tap `Handled`, turning a display-only row into one that swallows it.
    pub fn maybe_on_press(mut self, f: Option<impl Fn() + 'static>) -> Self {
        let Some(f) = f else { return self };
        self.press.set(f);
        self.mark_interactive();
        self
    }

    /// Registers this node in the interactive registry a click-through surface reads to carve its input region — see `StyledContainer::mark_interactive`.
    fn mark_interactive(&self) {
        crate::input_region::register_interactive(self.node, self.rect.read_only());
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
            // Neither will ever deliver the release a tap needs, and a press left armed pairs with whatever release arrives next and fires a click the user never made.
            Event::CursorLeft | Event::FocusChanged { is_focused: false } => {
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

impl Drop for Container {
    fn drop(&mut self) {
        crate::input_region::unregister_interactive(self.node);
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

    fn declaring_container(size: f32) -> Container {
        Container::new(LayoutStyle::new(), vec![])
            .unwrap()
            .declaring(move || Declared::default().with_font_size(size))
    }

    /// The first half of the bug `crate::context` describes: a declaration left behind lands on whatever the
    /// next tree builds on that id.
    ///
    /// Note what this does *not* call. `crate::context::reset_layout_runtime` sweeps the cascade too, and
    /// that pairing is the workaround — the reason the two "have to go together". Here only the layout half
    /// runs, so what keeps the second tree clean is the owner having withdrawn, not the reset.
    ///
    /// It also has to replace the runtime rather than merely drop and rebuild. Taffy's `NodeId` packs a
    /// slotmap version, so within one runtime a recycled slot yields a *different* id and the collision
    /// cannot happen. A fresh `LayoutRuntime` starts its versions over, which is what hands the second tree
    /// the first tree's ids exactly.
    #[test]
    fn a_second_tree_is_not_styled_by_what_the_first_declared() {
        reset_layout_runtime();
        let scope = reactive_core::owner_scope();
        let owner = scope.id();
        let first = declaring_container(11.0);
        let node = first.node;
        assert_eq!(crate::inherit::context(node).text.font_size, 11.0);

        drop(scope);
        reactive_core::dispose_owner(owner);
        drop(first);
        layout_reactive::reset_layout_runtime();

        let second = Container::new(LayoutStyle::new(), vec![]).unwrap();
        assert_eq!(second.node, node, "the second tree got the first tree's id");
        assert_eq!(
            crate::inherit::context(second.node).text.font_size,
            crate::inherit::Inherited::initial().text.font_size,
            "and nothing declared for it"
        );
    }

    /// The second half: a widget withdrawing one it no longer owns when it is finally dropped.
    ///
    /// Nothing about a late `Drop` needs a recycled id to be wrong — it only needs the declaration on that id
    /// to be somebody else's by then. Under the owner it withdraws nothing, because withdrawal is not its
    /// job any more.
    #[test]
    fn a_declaring_widget_dropped_late_withdraws_nothing() {
        reset_layout_runtime();
        let gone = reactive_core::owner_scope();
        let gone_owner = gone.id();
        let stale = declaring_container(11.0);
        let node = stale.node;
        drop(gone);
        reactive_core::dispose_owner(gone_owner);
        layout_reactive::reset_layout_runtime();

        let live = reactive_core::owner_scope();
        let current = declaring_container(22.0);
        assert_eq!(current.node, node, "built on the id the first one used");
        drop(live);

        drop(stale);
        assert_eq!(
            crate::inherit::context(node).text.font_size,
            22.0,
            "the late drop took the live declaration with it"
        );
    }

    fn make_container_with_labels() -> Container {
        reset_layout_runtime();
        let text_style = TextStyle::new(14.0, Color::WHITE);
        let text_a = Text::new(
            || "A".to_string(),
            LayoutStyle::new().width(50.0).height(20.0),
            {
                let text_style = text_style.clone();
                move || text_style.clone()
            },
        )
        .unwrap();
        let text_b = Text::new(
            || "B".to_string(),
            LayoutStyle::new().width(50.0).height(20.0),
            {
                let text_style = text_style.clone();
                move || text_style.clone()
            },
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
        let s_cb = s;
        // A pressable primitive stands in for the old high-level Button (now in ui-components).
        let btn = StyledContainer::new(
            LayoutStyle::new().width(50.0).height(30.0),
            |_r| renderer_core::RectStyle::default(),
            vec![],
        )
        .unwrap()
        .on_press(move || s_cb.update(|n| *n += 1));
        let btn_node = btn.layout_node();
        let s_txt = s;
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

    // A pressable plain Container must publish its rect to the same interactive registry StyledContainer uses, or a click-through surface never carves an input region for it.
    #[test]
    fn pressable_container_publishes_rect_to_interactive_registry_and_withdraws_on_drop() {
        use crate::interactive_rects;

        reset_layout_runtime();
        let baseline = interactive_rects().len();
        let container = Container::new(LayoutStyle::new().width(120.0).height(40.0), vec![])
            .unwrap()
            .on_press(|| {});
        let node = container.layout_node();
        assert_eq!(
            interactive_rects().len(),
            baseline,
            "an unlaid-out pressable contributes no rect"
        );
        compute_layout(
            node,
            AvailableSpace::Definite(120.0),
            AvailableSpace::Definite(40.0),
        )
        .unwrap();
        let rects = interactive_rects();
        assert_eq!(rects.len(), baseline + 1);
        assert!(
            rects.iter().any(|r| r.width == 120.0 && r.height == 40.0),
            "a laid-out pressable reports its rect"
        );
        drop(container);
        assert_eq!(
            interactive_rects().len(),
            baseline,
            "dropping the pressable withdraws its rect"
        );
    }

    #[test]
    fn container_can_be_nested_as_layout_item() {
        reset_layout_runtime();
        let inner = Container::column(vec![]).unwrap();
        let outer = Container::new(LayoutStyle::new().flex_row(), vec![Box::new(inner)]);
        assert!(outer.is_ok());
    }
}
