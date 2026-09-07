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

/// A panel floating over a pane covers it: a wheel that lands on the panel is not the pane's, whether or not the panel wants it. Without this the pane — declared first, painted underneath — takes the event that visually belongs to what is drawn on top of it.
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

/// **What is painted is what can be pressed.** An overlay hangs its marks from a box of no size, so the marks are laid out absolutely and drawn far from the parent that owns them. Hit-testing used to stop at the parent's own rect, so every one of them was visible and unreachable — and a `lazy` block, which is exactly such a box, took a whole editor's areas out of the pointer's reach without changing a pixel.
#[test]
fn a_box_of_no_size_does_not_swallow_what_it_holds() {
    use platform_core::PointerSource;
    reset_layout_runtime();
    let presses = Rc::new(Cell::new(0u32));
    let sink = presses.clone();
    // Placed absolutely, so it is drawn where the layout puts it and not inside the box that holds it.
    let mark = StyledContainer::new(
        LayoutStyle::new()
            .absolute()
            .inset_start(200.0)
            .inset_top(200.0)
            .width(40.0)
            .height(40.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .on_press(move || sink.set(sink.get() + 1));
    let layer = Container::new(
        LayoutStyle::new().absolute().width(0.0).height(0.0),
        vec![Box::new(mark)],
    )
    .unwrap();
    let mut root = Container::new(
        LayoutStyle::new().width(400.0).height(400.0),
        vec![Box::new(layer)],
    )
    .unwrap();
    compute_layout(
        root.layout_node(),
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();

    let press = |x: f64, y: f64| Event::PointerPressed {
        x,
        y,
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    };
    root.on_event(&press(220.0, 220.0));
    root.on_event(&Event::PointerReleased {
        x: 220.0,
        y: 220.0,
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    });
    assert_eq!(
        presses.get(),
        1,
        "the mark is where it is drawn, whatever the size of the box holding it"
    );
}

/// The same rule for hover: the pane still receives the move (a drag it started must keep tracking) but must not read a move over the panel as *the pointer is over me*.
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

/// A readout drawn over a pane is there to be *read*, not to be pointed at: `click_through` is the box saying so. Both halves of the rule have to let go of it — the wheel must reach the pane under it, and a move over it must still count as a move over the pane, or the operation the readout is describing stops the moment the pointer passes beneath it.
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

/// The other half of `click_through`, and the half that made it useless on its own: a control *inside* a click-through bar still hovers. The bar declining to shadow the pane is not the pane shadowing the bar — the bar is the one drawn on top. A floating toolbar over a canvas is exactly this shape, and without it none of its buttons could be pointed at.
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

/// Crossing the window border does not lift a button. A drag that outlives the border — which is the point of measuring one from its press — asks this registry which button started it on every move, and clearing here would answer "none" in the middle of the gesture. Losing the *focus* is the case where the release genuinely never arrives, and that one still clears.
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
    assert!(
        !pointer_buttons().any(),
        "the release outside the window still clears the button"
    );
}
