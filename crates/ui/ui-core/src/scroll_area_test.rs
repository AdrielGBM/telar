use crate::context::reset_layout_runtime;
use geometry_core::Rect;
use layout_core::{AvailableSpace, LayoutStyle, NodeId, SizeDimension};
use platform_core::{Event, PointerSource, ScrollDelta};
use renderer_core::DrawCommand;
use ui_tree::{Component, EventResult, RenderNode};

use super::*;
use crate::canvas::Canvas;
use crate::context::{compute_layout, new_container, track_layout};
use crate::layout_item::LayoutItem;
use crate::layout_leaf::LayoutLeaf;

// A `scroll` element has no other owner to compute its content, so laying out the node must lay out the detached subtree through its effect.
#[test]
fn scroll_area_lays_out_its_detached_content() {
    reset_layout_runtime();
    let content = Canvas::new(LayoutStyle::new().width(400.0).height(1000.0), |_| {
        RenderNode::Empty
    })
    .unwrap();
    let content_node = content.layout_node();
    let scroll = LayoutScrollArea::new(
        LayoutStyle::new().width(300.0).height(160.0),
        Box::new(content),
    )
    .unwrap();
    let scroll_node = scroll.layout_node();
    compute_layout(
        scroll_node,
        AvailableSpace::Definite(300.0),
        AvailableSpace::Definite(160.0),
    )
    .unwrap();
    let content_rect = track_layout(content_node).unwrap().get();
    assert!(
        content_rect.height > 0.0,
        "scroll must lay out its content, got {content_rect:?}"
    );
}

/// A page swapped for a shorter one must not leave the viewport looking at nothing.
///
/// This is the bug as a user meets it: scroll down a long page, switch to a short one, and the panel is blank — the offset is still 600 while there is 200 of content, so the transform has pushed all of it out of the clip. It comes back on the next wheel tick, which is what makes it read as a repaint bug rather than a scroll one. Nothing here touches an input event: the layout alone has to put it right.
#[test]
fn content_that_gets_shorter_pulls_the_view_back_into_it() {
    reset_layout_runtime();
    let tall = Canvas::new(LayoutStyle::new().width(300.0).height(900.0), |_| {
        RenderNode::Empty
    })
    .unwrap();
    let short = Canvas::new(LayoutStyle::new().width(300.0).height(120.0), |_| {
        RenderNode::Empty
    })
    .unwrap();
    let short_node = short.layout_node();
    let page = crate::container::Container::new(
        LayoutStyle::new().flex_column(),
        vec![Box::new(tall) as Box<dyn LayoutItem>],
    )
    .unwrap();
    let page_node = page.layout_node();
    let scroll = LayoutScrollArea::new(
        LayoutStyle::new().width(300.0).height(200.0),
        Box::new(page),
    )
    .unwrap();
    compute_layout(
        scroll.layout_node(),
        AvailableSpace::Definite(300.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();

    scroll.core.scroll_y.set(600.0);
    assert_eq!(scroll.core.scroll_y.get(), 600.0, "600 of 700 scrollable");

    crate::context::set_children(page_node, &[short_node]).unwrap();
    crate::context::mark_dirty(page_node).unwrap();
    crate::context::relayout_if_dirty();

    assert_eq!(
        scroll.core.scroll_y.get(),
        0.0,
        "a page shorter than the viewport has nothing to scroll, so the view is at its top — not \
         600px past the end of it, drawing nothing"
    );
}

/// A viewport command called from an effect must not subscribe that effect to the offset it writes.
///
/// This is how the fix for "the page changed, put the view back at the top" turned into "the page can no longer be scrolled at all": the effect noticing the page change also *read* the offset, so every wheel tick re-ran it, and it dutifully put the view back at the top. Both commands are `peek`-only for this reason, and both are exercised here — the launcher's follow-the-selection effect calls `reveal` from exactly the same place.
#[test]
fn a_viewport_command_run_from_an_effect_does_not_undo_the_users_own_scrolling() {
    reset_layout_runtime();
    let content = Canvas::new(LayoutStyle::new().width(300.0).height(900.0), |_| {
        RenderNode::Empty
    })
    .unwrap();
    let row = content.layout_node();
    let captured: Rc<RefCell<Option<ScrollViewport>>> = Rc::new(RefCell::new(None));
    let sink = Rc::clone(&captured);
    let scroll = LayoutScrollArea::new_with(
        LayoutStyle::new().width(300.0).height(200.0),
        move |viewport| {
            *sink.borrow_mut() = Some(viewport);
            Ok(Box::new(content) as Box<dyn LayoutItem>)
        },
    )
    .unwrap();
    compute_layout(
        scroll.layout_node(),
        AvailableSpace::Definite(300.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();
    let viewport = captured.borrow().clone().expect("the builder ran");

    let page = signal(0u32);
    let watched = page.read_only();
    let commanded = viewport.clone();
    let followed = watched;
    let _follow = effect(move || {
        followed.get();
        commanded.scroll_to_top();
    });

    scroll.core.scroll_y.set(150.0);
    reactive_core::batch(|| {});
    assert_eq!(
        scroll.core.scroll_y.get(),
        150.0,
        "scrolling is not a page change, so the effect must not have run and pulled the view back"
    );

    page.set(1);
    reactive_core::batch(|| {});
    assert_eq!(
        scroll.core.scroll_y.get(),
        0.0,
        "and the command still does its job when the thing it follows actually changes"
    );

    let revealing = viewport.clone();
    let _follow_row = effect(move || {
        watched.get();
        revealing.reveal(row, 4.0);
    });
    scroll.core.scroll_y.set(80.0);
    reactive_core::batch(|| {});
    assert_eq!(
        scroll.core.scroll_y.get(),
        80.0,
        "a row already in view is left alone, and scrolling must not re-ask about it"
    );
}

/// The same rule from the other side: the content stayed, the window grew.
#[test]
fn a_taller_viewport_pulls_the_view_back_into_the_content() {
    reset_layout_runtime();
    let content = Canvas::new(LayoutStyle::new().width(300.0).height(500.0), |_| {
        RenderNode::Empty
    })
    .unwrap();
    // Inside a column that fills the surface, which is how a page area gets its height: the viewport is whatever is left over, so resizing the surface resizes it.
    let scroll = LayoutScrollArea::new(
        LayoutStyle::new()
            .width(SizeDimension::Percent(1.0))
            .flex_grow(1.0)
            .min_height(0.0),
        Box::new(content),
    )
    .unwrap();
    let scroll_node = scroll.layout_node();
    let surface = new_container(
        LayoutStyle::new()
            .flex_column()
            .width(SizeDimension::Percent(1.0))
            .height(SizeDimension::Percent(1.0)),
        &[scroll_node],
    )
    .unwrap();
    compute_layout(
        surface,
        AvailableSpace::Definite(300.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();
    scroll.core.scroll_y.set(400.0);

    // The surface is resized taller — a float dragged by its grip, a monitor that changed mode.
    crate::context::mark_dirty(surface).unwrap();
    compute_layout(
        surface,
        AvailableSpace::Definite(300.0),
        AvailableSpace::Definite(450.0),
    )
    .unwrap();

    assert_eq!(
        scroll.core.scroll_y.get(),
        50.0,
        "500 of content in 450 of window leaves 50 to scroll, and that is where the view lands"
    );
}

/// A rebuilt tree — a shell following a config edit — is a second scroll area over the same offsets.
#[test]
fn a_scroll_built_on_kept_offsets_opens_where_the_last_one_left_off() {
    reset_layout_runtime();
    let offset = (signal(0.0f32), signal(0.0f32));
    let content = Canvas::new(LayoutStyle::new().width(300.0).height(900.0), |_| {
        RenderNode::Empty
    })
    .unwrap();
    let first = LayoutScrollArea::new_keeping(
        LayoutStyle::new().width(300.0).height(200.0),
        offset,
        |_| Ok(Box::new(content) as Box<dyn LayoutItem>),
    )
    .unwrap();
    compute_layout(
        first.layout_node(),
        AvailableSpace::Definite(300.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();
    first.core.scroll_y.set(320.0);
    drop(first);

    let content = Canvas::new(LayoutStyle::new().width(300.0).height(900.0), |_| {
        RenderNode::Empty
    })
    .unwrap();
    let rebuilt = LayoutScrollArea::new_keeping(
        LayoutStyle::new().width(300.0).height(200.0),
        offset,
        |_| Ok(Box::new(content) as Box<dyn LayoutItem>),
    )
    .unwrap();
    assert_eq!(
        rebuilt.core.scroll_y.get(),
        320.0,
        "the tree was replaced, the reader's place in it was not"
    );
    compute_layout(
        rebuilt.layout_node(),
        AvailableSpace::Definite(300.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();
    assert_eq!(
        rebuilt.core.scroll_y.get(),
        320.0,
        "and laying the rebuilt content out does not throw it away — a 0×0 rect is 'not measured yet', \
         not 'nothing to show'"
    );
}

// The end-to-end anchored-overlay case: a trigger deep inside a real scroll area, scrolled away from where it was laid out. Covers what the unit tests cannot — that the area registers its content rather than its viewport leaf, and that the subtree test reaches the separately-computed content root.
#[test]
fn a_trigger_scrolled_inside_a_scroll_area_anchors_where_it_is_drawn() {
    reset_layout_runtime();
    let spacer = Canvas::new(LayoutStyle::new().width(400.0).height(600.0), |_| {
        RenderNode::Empty
    })
    .unwrap();
    let trigger = Canvas::new(LayoutStyle::new().width(80.0).height(24.0), |_| {
        RenderNode::Empty
    })
    .unwrap();
    let trigger_node = trigger.layout_node();
    let content = crate::container::Container::new(
        LayoutStyle::new().flex_column(),
        vec![Box::new(spacer) as Box<dyn LayoutItem>, Box::new(trigger)],
    )
    .unwrap();
    let scroll = LayoutScrollArea::new(
        LayoutStyle::new().width(300.0).height(200.0),
        Box::new(content),
    )
    .unwrap();
    compute_layout(
        scroll.layout_node(),
        AvailableSpace::Definite(300.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();

    let laid_out = crate::context::absolute_rect(trigger_node).unwrap();
    assert!(laid_out.y > 200.0, "the trigger starts below the fold");
    assert_eq!(
        crate::scroll_region::visible_rect(trigger_node),
        Some(laid_out),
        "unscrolled, drawn position and laid-out position agree"
    );

    scroll.core.scroll_y.set(150.0);
    let drawn = crate::scroll_region::visible_rect(trigger_node).unwrap();
    assert_eq!(
        drawn.y,
        laid_out.y - 150.0,
        "scrolling down draws the trigger higher, and that is where a panel must anchor"
    );

    // Dropping the area withdraws its registration, so a later query is not shifted by a dead viewport.
    drop(scroll);
    assert_eq!(
        crate::scroll_region::visible_rect(trigger_node),
        Some(laid_out)
    );
}

pub(crate) fn make_scroll_area() -> ScrollArea {
    reset_layout_runtime();
    let content = Canvas::new(LayoutStyle::new().width(400.0).height(1000.0), |_| {
        RenderNode::Empty
    })
    .unwrap();
    let node = content.layout_node();
    let sa = ScrollArea::new(|| Rect::new(0.0, 0.0, 400.0, 300.0), Box::new(content));
    compute_layout(
        node,
        AvailableSpace::Definite(400.0),
        AvailableSpace::MaxContent,
    )
    .unwrap();
    sa
}

/// A wheel turn in the middle of the 400×300 viewport every fixture here uses.
fn settle() {
    let start = std::time::Instant::now();
    for frame in 1..=40 {
        motion_core::tick(start + std::time::Duration::from_millis(16 * frame));
    }
}

fn wheel_over(delta: ScrollDelta) -> Event {
    Event::Scrolled {
        delta,
        x: 100.0,
        y: 100.0,
    }
}

fn make_scroll_area_small() -> ScrollArea {
    reset_layout_runtime();
    let content = Canvas::new(LayoutStyle::new().width(400.0).height(200.0), |_| {
        RenderNode::Empty
    })
    .unwrap();
    let node = content.layout_node();
    let sa = ScrollArea::new(|| Rect::new(0.0, 0.0, 400.0, 300.0), Box::new(content));
    compute_layout(
        node,
        AvailableSpace::Definite(400.0),
        AvailableSpace::MaxContent,
    )
    .unwrap();
    sa
}

#[test]
fn a_click_inside_scrolled_content_that_rerenders_it_does_not_panic() {
    use crate::container::Container;
    use crate::context::track_layout;
    use crate::styled_container::StyledContainer;
    use platform_core::PointerButton;
    use reactive_core::{begin_batch, end_batch, signal};

    reset_layout_runtime();
    let s = signal(0i32);
    let s_cb = s;
    let btn = StyledContainer::new(
        LayoutStyle::new().width(50.0).height(30.0),
        |_r| RectStyle::default(),
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
    let content = Container::new(
        LayoutStyle::new().flex_column().width(400.0).height(1000.0),
        vec![Box::new(btn), Box::new(txt)],
    )
    .unwrap();
    let content_node = content.layout_node();
    let sa = ScrollArea::new(|| Rect::new(0.0, 0.0, 400.0, 300.0), Box::new(content));
    compute_layout(
        content_node,
        AvailableSpace::Definite(400.0),
        AvailableSpace::MaxContent,
    )
    .unwrap();
    let br = track_layout(btn_node).unwrap().get();

    let mut tree = crate::ComponentList::new(sa);
    let _ = tree.commands();

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
            end_batch();
            begin_batch();
        }
        let _ = tree.commands();
        end_batch();
    }
    assert_eq!(
        s.get(),
        1,
        "scroll-content click should increment the signal"
    );
}

// The area must cancel the pending tap once it detects the scroll, or the button fires.
#[test]
fn scroll_gesture_over_button_does_not_click() {
    use crate::container::Container;
    use crate::context::track_layout;
    use crate::styled_container::StyledContainer;
    use platform_core::PointerButton;
    use reactive_core::signal;

    reset_layout_runtime();
    let s = signal(0i32);
    let s_cb = s;
    let btn = StyledContainer::new(
        LayoutStyle::new().width(50.0).height(30.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .on_press(move || s_cb.update(|n| *n += 1));
    let btn_node = btn.layout_node();
    let content = Container::new(
        LayoutStyle::new().flex_column().width(400.0).height(1000.0),
        vec![Box::new(btn)],
    )
    .unwrap();
    let content_node = content.layout_node();
    let mut sa = ScrollArea::new(|| Rect::new(0.0, 0.0, 400.0, 300.0), Box::new(content));
    compute_layout(
        content_node,
        AvailableSpace::Definite(400.0),
        AvailableSpace::MaxContent,
    )
    .unwrap();
    let br = track_layout(btn_node).unwrap().get();
    let (cx, cy) = (
        (br.x + br.width / 2.0) as f64,
        (br.y + br.height / 2.0) as f64,
    );

    sa.on_event(&Event::PointerPressed {
        x: cx,
        y: cy,
        button: PointerButton::Primary,
        source: PointerSource::Touch { id: 1 },
    });
    for _ in 0..5 {
        sa.on_event(&Event::Scrolled {
            delta: ScrollDelta::Pixels { x: 0.0, y: -20.0 },
            x: cx,
            y: cy,
        });
        sa.on_event(&Event::PointerMoved {
            x: cx,
            y: cy,
            source: PointerSource::Touch { id: 1 },
        });
    }
    sa.on_event(&Event::PointerReleased {
        x: cx,
        y: cy,
        button: PointerButton::Primary,
        source: PointerSource::Touch { id: 1 },
    });

    assert_eq!(
        s.get(),
        0,
        "a scroll gesture over a button must not click it"
    );
    assert!(
        sa.core.scroll_y.get() > 0.0,
        "the gesture should have scrolled the content"
    );
}

#[test]
fn as_layout_item_uses_leaf_rect_as_viewport() {
    reset_layout_runtime();
    let content = Canvas::new(LayoutStyle::new().width(400.0).height(1000.0), |_| {
        RenderNode::Empty
    })
    .unwrap();
    let content_node = content.layout_node();
    let sa = LayoutScrollArea::new(
        LayoutStyle::new().width(400.0).height(300.0),
        Box::new(content),
    )
    .unwrap();
    let root = new_container(
        LayoutStyle::new().flex_column().width(400.0).height(300.0),
        &[sa.layout_node()],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(300.0),
    )
    .unwrap();
    compute_layout(
        content_node,
        AvailableSpace::Definite(400.0),
        AvailableSpace::MaxContent,
    )
    .unwrap();
    let vp = sa.viewport_rect();
    assert_eq!(vp.width, 400.0);
    assert_eq!(vp.height, 300.0);
}

#[test]
fn as_layout_item_emits_clip_and_vbar_on_overflow() {
    reset_layout_runtime();
    let content = Canvas::new(LayoutStyle::new().width(400.0).height(1000.0), |_| {
        RenderNode::Empty
    })
    .unwrap();
    let content_node = content.layout_node();
    let sa = LayoutScrollArea::new(
        LayoutStyle::new().width(400.0).height(300.0),
        Box::new(content),
    )
    .unwrap();
    let root = new_container(
        LayoutStyle::new().flex_column().width(400.0).height(300.0),
        &[sa.layout_node()],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(300.0),
    )
    .unwrap();
    compute_layout(
        content_node,
        AvailableSpace::Definite(400.0),
        AvailableSpace::MaxContent,
    )
    .unwrap();
    if let RenderNode::Group { children, .. } = sa.view() {
        assert_eq!(children.len(), 3);
        assert!(
            matches!(&children[0], RenderNode::Clip { .. }),
            "overflowing content is clipped"
        );
        assert!(
            matches!(
                &children[1],
                RenderNode::Primitive(DrawCommand::Rect { .. })
            ),
            "and gets a vertical bar"
        );
    } else {
        panic!("expected Group");
    }
}

#[test]
fn scroll_lines_updates_offset() {
    let mut sa = make_scroll_area();
    sa.on_event(&wheel_over(ScrollDelta::Lines { x: 0.0, y: -3.0 }));
    settle();
    assert_eq!(sa.core.scroll_y.get(), 60.0);
}

// A wheel outside this viewport belongs to an ancestor or to an inner scroll that consumed it.
#[test]
fn wheel_outside_viewport_does_not_scroll() {
    let mut sa = make_scroll_area(); // viewport 400x300
    let result = sa.on_event(&Event::Scrolled {
        delta: ScrollDelta::Lines { x: 0.0, y: -3.0 },
        x: 500.0,
        y: 500.0,
    });
    assert_eq!(result, EventResult::Ignored);
    assert_eq!(
        sa.core.scroll_y.get(),
        0.0,
        "must not scroll when the wheel turned elsewhere"
    );
}

/// The wheel carries where it happened, so the first one lands correctly even though the pointer has never moved — the case a viewport that tracked moves itself could only guess at.
#[test]
fn wheel_inside_viewport_scrolls_without_a_prior_move() {
    let mut sa = make_scroll_area();
    sa.on_event(&wheel_over(ScrollDelta::Lines { x: 0.0, y: -3.0 }));
    settle();
    assert!(
        sa.core.scroll_y.get() > 0.0,
        "wheel over the viewport scrolls it"
    );
}

/// What a terminal does: its smallest visible step is a whole cell, so the notch is applied at once.
#[test]
fn a_surface_that_cannot_show_the_gap_takes_the_notch_whole() {
    let was = ui_tree::set_smooth_wheel(false);
    let mut sa = make_scroll_area();
    sa.on_event(&wheel_over(ScrollDelta::Lines { x: 0.0, y: -3.0 }));
    assert_eq!(sa.core.scroll_y.get(), 60.0, "no frame clock involved");
    ui_tree::set_smooth_wheel(was);
}

/// A finger landing on a laptop that has both a wheel and a touch screen takes the offset over.
#[test]
fn a_press_catches_a_notch_still_being_covered() {
    let mut sa = make_scroll_area();
    sa.on_event(&wheel_over(ScrollDelta::Lines { x: 0.0, y: -3.0 }));
    let start = std::time::Instant::now();
    motion_core::tick(start);
    motion_core::tick(start + std::time::Duration::from_millis(16));
    let caught = sa.core.scroll_y.get();
    assert!((0.0..60.0).contains(&caught), "partway there: {caught}");
    sa.on_event(&Event::PointerPressed {
        x: 10.0,
        y: 10.0,
        button: platform_core::PointerButton::Primary,
        source: platform_core::PointerSource::Mouse,
    });
    settle();
    assert_eq!(sa.core.scroll_y.get(), caught, "the press is in charge now");
}

#[test]
fn scroll_pixels_updates_offset() {
    let mut sa = make_scroll_area();
    sa.on_event(&wheel_over(ScrollDelta::Pixels { x: 0.0, y: -80.0 }));
    assert_eq!(sa.core.scroll_y.get(), 80.0);
}

#[test]
fn scroll_clamps_to_max() {
    let mut sa = make_scroll_area();
    sa.on_event(&wheel_over(ScrollDelta::Pixels { x: 0.0, y: -9999.0 }));
    assert_eq!(sa.core.scroll_y.get(), 700.0);
}

#[test]
fn scroll_clamps_to_zero() {
    let mut sa = make_scroll_area();
    sa.on_event(&wheel_over(ScrollDelta::Pixels { x: 0.0, y: 9999.0 }));
    assert_eq!(sa.core.scroll_y.get(), 0.0);
}

#[test]
fn pointer_outside_viewport_is_ignored() {
    let mut sa = make_scroll_area();
    let result = sa.on_event(&Event::PointerMoved {
        x: 500.0,
        y: 100.0,
        source: PointerSource::Mouse,
    });
    assert!(
        matches!(result, EventResult::Ignored),
        "a pointer outside the viewport is not this area's business"
    );
}

#[test]
fn view_emits_clip_and_scrollbar_when_content_overflows() {
    let sa = make_scroll_area();
    let view = sa.view();
    if let RenderNode::Group { children, .. } = view {
        assert_eq!(children.len(), 3);
        assert!(
            matches!(&children[0], RenderNode::Clip { .. }),
            "overflowing content is clipped"
        );
        assert!(
            matches!(
                &children[1],
                RenderNode::Primitive(DrawCommand::Rect { .. })
            ),
            "and gets a scrollbar"
        );
    } else {
        panic!("expected Group");
    }
}

#[test]
fn view_no_scrollbar_when_content_fits() {
    let sa = make_scroll_area_small();
    let view = sa.view();
    if let RenderNode::Group { children, .. } = view {
        assert!(
            matches!(&children[1], RenderNode::Empty),
            "content that fits needs no scrollbar"
        );
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

    reset_layout_runtime();
    let leaf = LayoutLeaf::register(LayoutStyle::new().width(400.0).height(1000.0)).unwrap();
    let node = leaf.node;
    let content = CapturingItem {
        leaf,
        out: captured_y_clone,
    };
    let mut sa = ScrollArea::new(|| Rect::new(100.0, 50.0, 400.0, 300.0), Box::new(content));
    compute_layout(
        node,
        AvailableSpace::Definite(400.0),
        AvailableSpace::MaxContent,
    )
    .unwrap();

    sa.on_event(&Event::Scrolled {
        delta: ScrollDelta::Pixels { x: 0.0, y: -100.0 },
        x: 150.0,
        y: 200.0,
    });

    sa.on_event(&Event::PointerMoved {
        x: 150.0,
        y: 200.0,
        source: PointerSource::Mouse,
    });

    assert!(
        (captured_y.get() - 250.0).abs() < 0.001,
        "the node was laid out at the scrolled offset: {}",
        captured_y.get()
    );
}

pub(crate) fn make_scroll_area_wide() -> ScrollArea {
    reset_layout_runtime();
    let content = Canvas::new(LayoutStyle::new().width(1000.0).height(300.0), |_| {
        RenderNode::Empty
    })
    .unwrap();
    let node = content.layout_node();
    let sa = ScrollArea::new(|| Rect::new(0.0, 0.0, 400.0, 300.0), Box::new(content));
    compute_layout(
        node,
        AvailableSpace::Definite(1000.0),
        AvailableSpace::MaxContent,
    )
    .unwrap();
    sa
}

#[test]
fn scroll_x_lines_updates_offset() {
    let mut sa = make_scroll_area_wide();
    sa.on_event(&wheel_over(ScrollDelta::Lines { x: -3.0, y: 0.0 }));
    settle();
    assert_eq!(sa.core.scroll_x.get(), 60.0);
}

#[test]
fn scroll_x_pixels_updates_offset() {
    let mut sa = make_scroll_area_wide();
    sa.on_event(&wheel_over(ScrollDelta::Pixels { x: -80.0, y: 0.0 }));
    assert_eq!(sa.core.scroll_x.get(), 80.0);
}

#[test]
fn scroll_x_clamps_to_max() {
    let mut sa = make_scroll_area_wide();
    sa.on_event(&wheel_over(ScrollDelta::Pixels { x: -9999.0, y: 0.0 }));
    assert_eq!(sa.core.scroll_x.get(), 600.0);
}

#[test]
fn scroll_x_clamps_to_zero() {
    let mut sa = make_scroll_area_wide();
    sa.on_event(&wheel_over(ScrollDelta::Pixels { x: 9999.0, y: 0.0 }));
    assert_eq!(sa.core.scroll_x.get(), 0.0);
}

#[test]
fn view_emits_hbar_when_content_overflows_x() {
    let sa = make_scroll_area_wide();
    let view = sa.view();
    if let RenderNode::Group { children, .. } = view {
        assert_eq!(children.len(), 3);
        assert!(
            matches!(&children[0], RenderNode::Clip { .. }),
            "overflowing content is clipped"
        );
        assert!(
            matches!(&children[1], RenderNode::Empty),
            "nothing overflows vertically, so no vertical bar"
        );
        assert!(
            matches!(
                &children[2],
                RenderNode::Primitive(DrawCommand::Rect { .. })
            ),
            "but the horizontal one is there"
        );
    } else {
        panic!("expected Group");
    }
}

#[test]
fn view_no_hbar_when_content_fits_x() {
    let sa = make_scroll_area();
    let view = sa.view();
    if let RenderNode::Group { children, .. } = view {
        assert_eq!(children.len(), 3);
        assert!(
            matches!(&children[2], RenderNode::Empty),
            "content that fits horizontally needs no horizontal bar"
        );
    } else {
        panic!("expected Group");
    }
}
