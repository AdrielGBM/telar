use std::cell::Cell;

use geometry_core::Rect;
use platform_core::{Event, PointerButton};
use ui_tree::EventResult;

/// Which pointer buttons are held right now.
///
/// The pointer's half of [`crate::modifiers`], and there for the same reason: a gesture that behaves one way
/// per button has to ask, and the callbacks it is written against report *where* the pointer is, not *what*
/// started it. A modeller is the case — drag to orbit, right-drag to pan — and widening `on_drag` to carry a
/// button would make the whole catalogue pay for a question two widgets ask.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PointerButtons {
    pub primary: bool,
    pub secondary: bool,
    pub auxiliary: bool,
}

impl PointerButtons {
    /// Whether any button at all is down.
    pub fn any(self) -> bool {
        self.primary || self.secondary || self.auxiliary
    }

    fn slot(&mut self, button: &PointerButton) -> &mut bool {
        match button {
            PointerButton::Primary => &mut self.primary,
            PointerButton::Secondary => &mut self.secondary,
            PointerButton::Auxiliary => &mut self.auxiliary,
        }
    }

    pub(crate) fn holds(self, button: &PointerButton) -> bool {
        match button {
            PointerButton::Primary => self.primary,
            PointerButton::Secondary => self.secondary,
            PointerButton::Auxiliary => self.auxiliary,
        }
    }

    pub(crate) fn with(mut self, button: &PointerButton) -> Self {
        *self.slot(button) = true;
        self
    }
}

thread_local! {
    /// Set while a move is being dispatched into a subtree that something else is drawn over. See
    /// [`pointer_occluded`].
    static OCCLUDED: Cell<bool> = const { Cell::new(false) };
    static BUTTONS: Cell<PointerButtons> = const {
        Cell::new(PointerButtons { primary: false, secondary: false, auxiliary: false })
    };
}

/// Records what `event` says about the buttons. The runner calls this for every event before dispatch, so a
/// handler running on this very event already sees the state it establishes.
pub fn observe_pointer(event: &Event) {
    BUTTONS.with(|b| {
        let mut held = b.get();
        match event {
            Event::PointerPressed { button, .. } => *held.slot(button) = true,
            Event::PointerReleased { button, .. } => *held.slot(button) = false,
            // A window that loses focus never sends the releases for what was down. `CursorLeft` is deliberately not here: crossing the border does not lift a button, and forgetting it would leave a live drag unable to say which button started it.
            Event::FocusChanged { is_focused: false } => held = PointerButtons::default(),
            _ => return,
        }
        b.set(held);
    });
}

/// The pointer buttons held right now.
pub fn pointer_buttons() -> PointerButtons {
    BUTTONS.with(|b| b.get())
}

/// Drops the button state; parallels the other per-tree resets on teardown and hot reload.
pub fn reset_pointer() {
    BUTTONS.with(|b| b.set(PointerButtons::default()));
}

/// Whether the move being dispatched right now landed on something drawn in front of this widget.
///
/// A move is broadcast to every child, not only the one under the pointer, because a widget that armed a
/// press or began a drag has to keep receiving them after the pointer leaves its box (pointer capture). That
/// is right for the gesture and wrong for hover: two overlapping boxes would both read the same move as
/// *the pointer is over me*, and a viewport would highlight the face behind the panel the user is pointing
/// at. The container marks the covered subtrees as it broadcasts, and the widgets that track hover ask here.
pub(crate) fn pointer_occluded() -> bool {
    OCCLUDED.with(|c| c.get())
}

struct OccludedGuard(bool);

impl Drop for OccludedGuard {
    fn drop(&mut self) {
        OCCLUDED.with(|c| c.set(self.0));
    }
}

/// Marks everything dispatched until the guard drops as covered. Set-only on the way down: a subtree inside
/// something covered is covered too, whatever its own children are stacked like.
fn occlude() -> OccludedGuard {
    OccludedGuard(OCCLUDED.with(|c| c.replace(true)))
}

pub(crate) fn pointer_coords(event: &Event) -> Option<(f64, f64)> {
    match event {
        Event::PointerMoved { x, y, .. } => Some((*x, *y)),
        Event::PointerPressed { x, y, .. } => Some((*x, *y)),
        Event::PointerReleased { x, y, .. } => Some((*x, *y)),
        Event::Scrolled { x, y, .. } => Some((*x, *y)),
        _ => None,
    }
}

/// Applies the full affine inverse of `matrix` to all pointer-coordinate events. Returns `None` for
/// non-pointer events or when `matrix` is degenerate (det ≈ 0), so callers fall back to the original.
///
/// Public because a component that paints a subtree under a [`RenderNode::Transform`] it chose itself — a
/// hand-placed rail or panel, rather than a laid-out one — has to put the same transform's inverse on the
/// events it forwards there, or its hit-testing drifts from what is on screen.
pub fn transform_pointer(event: &Event, matrix: [f32; 6]) -> Option<Event> {
    let inv = geometry_core::Transform::from_array(matrix).invert()?;
    // Map in f64 so pointer coordinates keep their precision; Transform::apply would round-trip through f32.
    let apply = |world_x: f64, world_y: f64| -> (f64, f64) {
        let local_x = inv.a as f64 * world_x + inv.c as f64 * world_y + inv.e as f64;
        let local_y = inv.b as f64 * world_x + inv.d as f64 * world_y + inv.f as f64;
        (local_x, local_y)
    };
    match event {
        Event::PointerMoved { x, y, source } => {
            let (local_x, local_y) = apply(*x, *y);
            Some(Event::PointerMoved {
                x: local_x,
                y: local_y,
                source: source.clone(),
            })
        }
        Event::PointerPressed {
            x,
            y,
            button,
            source,
        } => {
            let (local_x, local_y) = apply(*x, *y);
            Some(Event::PointerPressed {
                x: local_x,
                y: local_y,
                button: *button,
                source: source.clone(),
            })
        }
        Event::PointerReleased {
            x,
            y,
            button,
            source,
        } => {
            let (local_x, local_y) = apply(*x, *y);
            Some(Event::PointerReleased {
                x: local_x,
                y: local_y,
                button: *button,
                source: source.clone(),
            })
        }
        // The delta is untouched: it is a distance in wheel notches or screen pixels, not a point in the
        // space this maps out of. Only where the wheel turned moves with the subtree.
        Event::Scrolled { delta, x, y } => {
            let (local_x, local_y) = apply(*x, *y);
            Some(Event::Scrolled {
                delta: delta.clone(),
                x: local_x,
                y: local_y,
            })
        }
        _ => None,
    }
}

pub(crate) fn offset_pointer(event: &Event, dx: f64, dy: f64) -> Option<Event> {
    transform_pointer(event, [1.0, 0.0, 0.0, 1.0, dx as f32, dy as f32])
}

// Returns a reference to `event` when the pointer is inside `rect`, or None when it is outside. Non-pointer events always pass through (returns Some). Callers use None to short-circuit to Ignored.
pub(crate) fn clip_pointer_event<'a>(event: &'a Event, rect: Rect) -> Option<&'a Event> {
    match pointer_coords(event) {
        Some((x, y)) if !rect.contains(x as f32, y as f32) => None,
        _ => Some(event),
    }
}

pub(crate) fn dispatch_container_event(
    children: &mut crate::layout_item::TrackedChildren,
    event: &Event,
) -> EventResult {
    let _dispatching = crate::disposal::dispatching();
    // Moves AND releases broadcast to every child regardless of position: a widget that armed a press or
    // began a drag inside its bounds must still receive the release even when the pointer has since moved
    // outside (pointer-capture semantics). Each widget's release handler is guarded by its own armed/drag
    // state, so broadcasting never double-fires an unrelated widget. Hit-testing (below) applies to presses
    // and to the wheel, where the target is chosen by where the pointer is.
    if matches!(
        event,
        Event::PointerMoved { .. } | Event::PointerReleased { .. }
    ) {
        // The topmost child containing the point is the one the pointer is *over*; every other child is
        // dispatched the same move (its gesture may still be running) but under the occlusion mark.
        let over = pointer_coords(event).and_then(|(x, y)| {
            children.iter().rposition(|c| {
                c.rect
                    .as_ref()
                    .is_some_and(|sig| sig.get().contains(x as f32, y as f32))
                    && c.item.borrow().pointer_opaque()
            })
        });
        let mut any_handled = false;
        for (i, child) in children.iter().enumerate() {
            // Covered means something is drawn *over* it, so only a later sibling occludes an earlier one.
            // Comparing for inequality instead marked the children drawn on top as covered too, which is
            // invisible while every sibling is opaque — the topmost is always the last — and wrong the moment
            // one is not: a `click_through` bar declines to shadow the pane under it and was then told the
            // pane was shadowing *it*, so nothing inside it could be hovered.
            let _covered = (over.is_some_and(|top| top > i)).then(occlude);
            if child.owning(|| child.item.borrow_mut().on_event(event)) == EventResult::Handled {
                any_handled = true;
            }
        }
        return if any_handled {
            EventResult::Handled
        } else {
            EventResult::Ignored
        };
    }
    let Some((x, y)) = pointer_coords(event).map(|(x, y)| (x as f32, y as f32)) else {
        return dispatch_to_children(children, event);
    };
    // Back to front, because that is the order they are painted in: where two children overlap, the one
    // drawn on top is the one the user aimed at, and it takes the event whether or not it wants it — a box
    // covers what is behind it, exactly as a browser hit-tests. Falling sideways to a covered sibling is
    // what made a wheel over a floating panel zoom the pane underneath it. In flow layout siblings cannot
    // overlap and none of this is observable; `absolute` is what makes it real.
    for child in children.iter_mut().rev() {
        // A child with no laid-out rect cannot be hit-tested, so it is offered the event but never blocks.
        let rect = child.rect.as_ref().map(|sig| sig.get());
        if !rect.is_none_or(|r| r.contains(x, y)) {
            continue;
        }
        let result = child.owning(|| child.item.borrow_mut().on_event(event));
        // A widget that is not there for hit-testing purposes (an overlay, routed by its own registry) lets
        // the search carry on to whatever it was drawn over.
        if result == EventResult::Handled
            || (rect.is_some() && child.item.borrow().pointer_opaque())
        {
            return result;
        }
    }
    EventResult::Ignored
}

fn dispatch_to_children(
    children: &mut crate::layout_item::TrackedChildren,
    event: &Event,
) -> EventResult {
    for child in children.iter_mut() {
        if child.owning(|| child.item.borrow_mut().on_event(event)) == EventResult::Handled {
            return EventResult::Handled;
        }
    }
    EventResult::Ignored
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container::Container;
    use crate::context::{compute_layout, reset_layout_runtime};
    use crate::layout_item::LayoutItem;
    use crate::styled_container::StyledContainer;
    use layout_core::{AvailableSpace, LayoutStyle};
    use platform_core::ScrollDelta;
    use renderer_core::RectStyle;
    use std::cell::Cell;
    use std::rc::Rc;
    use ui_tree::Component;

    /// A panel floating over a pane covers it: a wheel that lands on the panel is not the pane's, whether or
    /// not the panel wants it. Without this the pane — declared first, painted underneath — takes the event
    /// that visually belongs to what is drawn on top of it.
    #[test]
    fn a_covering_sibling_takes_the_pointer_from_the_one_beneath() {
        reset_layout_runtime();
        let pane_wheels = Rc::new(Cell::new(0u32));
        let sink = pane_wheels.clone();
        let pane = StyledContainer::new(
            LayoutStyle::new().width(400.0).height(400.0),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .on_scroll(move |_dx, _dy| sink.set(sink.get() + 1));
        // Declared after the pane and out of flow, so it is painted over it rather than beside it.
        let panel = Container::new(
            LayoutStyle::new()
                .absolute()
                .inset_end(0.0)
                .inset_top(0.0)
                .width(100.0)
                .height(400.0),
            vec![],
        )
        .unwrap();
        let mut root = Container::new(
            LayoutStyle::new().flex_row().width(400.0).height(400.0),
            vec![Box::new(pane), Box::new(panel)],
        )
        .unwrap();
        compute_layout(
            root.layout_node(),
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(400.0),
        )
        .unwrap();

        let wheel = |x: f64| Event::Scrolled {
            delta: ScrollDelta::Lines { x: 0.0, y: -3.0 },
            x,
            y: 200.0,
        };
        root.on_event(&wheel(50.0));
        assert_eq!(pane_wheels.get(), 1, "over the pane it is the pane's");
        root.on_event(&wheel(350.0));
        assert_eq!(
            pane_wheels.get(),
            1,
            "over the panel it is the panel's, even though the panel ignored it"
        );
    }

    /// The same rule for hover: the pane still receives the move (a drag it started must keep tracking) but
    /// must not read a move over the panel as *the pointer is over me*.
    #[test]
    fn a_covered_pane_is_not_hovered_by_a_move_over_the_panel() {
        use platform_core::PointerSource;
        reset_layout_runtime();
        let at: Rc<Cell<Option<(f32, f32)>>> = Rc::new(Cell::new(None));
        let sink = at.clone();
        let pane = StyledContainer::new(
            LayoutStyle::new().width(400.0).height(400.0),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .on_pointer_move(move |x, y| sink.set(Some((x, y))));
        let panel = Container::new(
            LayoutStyle::new()
                .absolute()
                .inset_end(0.0)
                .inset_top(0.0)
                .width(100.0)
                .height(400.0),
            vec![],
        )
        .unwrap();
        let mut root = Container::new(
            LayoutStyle::new().flex_row().width(400.0).height(400.0),
            vec![Box::new(pane), Box::new(panel)],
        )
        .unwrap();
        compute_layout(
            root.layout_node(),
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(400.0),
        )
        .unwrap();

        let moved = |x: f64| Event::PointerMoved {
            x,
            y: 200.0,
            source: PointerSource::Mouse,
        };
        root.on_event(&moved(50.0));
        assert_eq!(at.get(), Some((50.0, 200.0)), "over the pane it tracks");
        at.set(None);
        root.on_event(&moved(350.0));
        assert_eq!(at.get(), None, "the panel is in front of it there");
    }

    /// A readout drawn over a pane is there to be *read*, not to be pointed at: `click_through` is the box
    /// saying so. Both halves of the rule have to let go of it — the wheel must reach the pane under it, and
    /// a move over it must still count as a move over the pane, or the operation the readout is describing
    /// stops the moment the pointer passes beneath it.
    #[test]
    fn a_click_through_label_does_not_stand_between_the_pointer_and_the_pane() {
        use platform_core::PointerSource;
        reset_layout_runtime();
        let wheels = Rc::new(Cell::new(0u32));
        let at: Rc<Cell<Option<(f32, f32)>>> = Rc::new(Cell::new(None));
        let (wheel_sink, move_sink) = (wheels.clone(), at.clone());
        let pane = StyledContainer::new(
            LayoutStyle::new().width(400.0).height(400.0),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .on_scroll(move |_dx, _dy| wheel_sink.set(wheel_sink.get() + 1))
        .on_pointer_move(move |x, y| move_sink.set(Some((x, y))));
        let readout = StyledContainer::new(
            LayoutStyle::new()
                .absolute()
                .inset_end(0.0)
                .inset_top(0.0)
                .width(100.0)
                .height(400.0),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .click_through(true);
        let mut root = Container::new(
            LayoutStyle::new().flex_row().width(400.0).height(400.0),
            vec![Box::new(pane), Box::new(readout)],
        )
        .unwrap();
        compute_layout(
            root.layout_node(),
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(400.0),
        )
        .unwrap();

        root.on_event(&Event::Scrolled {
            delta: ScrollDelta::Lines { x: 0.0, y: -3.0 },
            x: 350.0,
            y: 200.0,
        });
        assert_eq!(wheels.get(), 1, "the wheel reached the pane underneath");
        root.on_event(&Event::PointerMoved {
            x: 350.0,
            y: 200.0,
            source: PointerSource::Mouse,
        });
        assert_eq!(
            at.get(),
            Some((350.0, 200.0)),
            "and the pane is still the thing the pointer is over"
        );
    }

    /// The other half of `click_through`, and the half that made it useless on its own: a control *inside* a
    /// click-through bar still hovers. The bar declining to shadow the pane is not the pane shadowing the bar
    /// — the bar is the one drawn on top. A floating toolbar over a canvas is exactly this shape, and without
    /// it none of its buttons could be pointed at.
    #[test]
    fn a_control_inside_a_click_through_bar_is_still_hovered() {
        use platform_core::PointerSource;
        reset_layout_runtime();
        let pane_at: Rc<Cell<Option<(f32, f32)>>> = Rc::new(Cell::new(None));
        let button_at: Rc<Cell<Option<(f32, f32)>>> = Rc::new(Cell::new(None));
        let (pane_sink, button_sink) = (pane_at.clone(), button_at.clone());
        let pane = StyledContainer::new(
            LayoutStyle::new().width(400.0).height(400.0),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .on_pointer_move(move |x, y| pane_sink.set(Some((x, y))));
        let button = StyledContainer::new(
            LayoutStyle::new().width(60.0).height(30.0),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .on_pointer_move(move |x, y| button_sink.set(Some((x, y))));
        let bar = StyledContainer::new(
            LayoutStyle::new()
                .absolute()
                .inset_top(0.0)
                .width(400.0)
                .height(40.0),
            |_r| RectStyle::default(),
            vec![Box::new(button)],
        )
        .unwrap()
        .click_through(true);
        let mut root = Container::new(
            LayoutStyle::new().flex_row().width(400.0).height(400.0),
            vec![Box::new(pane), Box::new(bar)],
        )
        .unwrap();
        compute_layout(
            root.layout_node(),
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(400.0),
        )
        .unwrap();

        root.on_event(&Event::PointerMoved {
            x: 30.0,
            y: 15.0,
            source: PointerSource::Mouse,
        });
        assert_eq!(
            button_at.get(),
            Some((30.0, 15.0)),
            "the button in the bar is hovered"
        );
        assert_eq!(
            pane_at.get(),
            Some((30.0, 15.0)),
            "and the pane under it goes on tracking"
        );
    }

    /// Crossing the window border does not lift a button. A drag that outlives the border — which is the
    /// point of measuring one from its press — asks this registry which button started it on every move, and
    /// clearing here would answer "none" in the middle of the gesture. Losing the *focus* is the case where
    /// the release genuinely never arrives, and that one still clears.
    #[test]
    fn cursor_leaving_the_window_does_not_forget_a_held_button() {
        use platform_core::{PointerButton, PointerSource};

        reset_pointer();
        observe_pointer(&Event::PointerPressed {
            x: 10.0,
            y: 10.0,
            button: PointerButton::Secondary,
            source: PointerSource::Mouse,
        });
        observe_pointer(&Event::CursorLeft);
        assert!(
            pointer_buttons().secondary,
            "the button that armed the drag is still down"
        );

        observe_pointer(&Event::FocusChanged { is_focused: false });
        assert!(!pointer_buttons().any());
    }
}
