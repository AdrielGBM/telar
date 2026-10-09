use std::cell::Cell;
use std::rc::Rc;

use geometry_core::{Rect, Size};
use layout_core::{AvailableSpace, LayoutStyle, NodeId, SizeDimension};
use platform_core::{PointerButton, PointerSource};
use renderer_core::{DrawCommand, ElementId};
use ui_tree::SegmentRoot;

use super::*;
use crate::canvas::Canvas;
use crate::container::Container;
use crate::context::{
    compute_layout, relayout_if_dirty, reset_layout_runtime, set_surface_size, track_layout,
};
use crate::{ScrollPage, StyledContainer, dispatch_overlays, focus, visible_rect};

const SURFACE: Size = Size {
    width: 800.0,
    height: 600.0,
};

fn block(style: LayoutStyle) -> Box<dyn LayoutItem> {
    Box::new(Canvas::new(style, |_| RenderNode::Empty).unwrap())
}

/// A 48px bar across the surface that counts the taps it gets.
fn bar(taps: Rc<Cell<u32>>) -> Container {
    Container::new(
        LayoutStyle::new()
            .width(SizeDimension::Percent(1.0))
            .height(48.0),
        vec![],
    )
    .unwrap()
    .on_press(move || taps.set(taps.get() + 1))
}

fn press(x: f64, y: f64) -> Event {
    Event::PointerPressed {
        x,
        y,
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    }
}

fn release(x: f64, y: f64) -> Event {
    Event::PointerReleased {
        x,
        y,
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    }
}

/// A page 2000px tall holding the layer near its top, laid out against the surface.
fn page_with_layer(taps: Rc<Cell<u32>>) -> (ScrollPage, NodeId) {
    reset_layout_runtime();
    set_surface_size(SURFACE);
    let bar = bar(taps);
    let bar_node = bar.layout_node();
    let mut page = ScrollPage::new_with(|_| {
        let layer = FixedLayer::new(LayoutStyle::new().flex_column(), vec![Box::new(bar)])?;
        let content = Container::new(
            LayoutStyle::new().flex_column(),
            vec![Box::new(layer), block(LayoutStyle::new().height(2000.0))],
        )?;
        Ok(Box::new(content))
    })
    .unwrap();
    page.relayout(SURFACE.width, SURFACE.height);
    relayout_if_dirty();
    (page, bar_node)
}

#[test]
fn a_layer_built_before_any_root_stands_against_the_surface_from_the_first_frame() {
    reset_layout_runtime();
    set_surface_size(SURFACE);
    let bar = bar(Rc::default());
    let bar_node = bar.layout_node();
    let _layer = FixedLayer::new(LayoutStyle::new().flex_column(), vec![Box::new(bar)]).unwrap();

    relayout_if_dirty();
    assert_eq!(
        track_layout(bar_node).unwrap().get(),
        Rect::new(0.0, 0.0, 800.0, 48.0)
    );

    set_surface_size(Size::new(1000.0, 600.0));
    relayout_if_dirty();
    assert_eq!(
        track_layout(bar_node).unwrap().get().width,
        1000.0,
        "a resized surface lays the layer out again"
    );
}

#[test]
fn where_it_is_declared_it_takes_no_room_and_adds_no_gap() {
    reset_layout_runtime();
    set_surface_size(SURFACE);
    let after = block(LayoutStyle::new().height(100.0));
    let after_node = after.layout_node();
    let layer = FixedLayer::new(
        LayoutStyle::new(),
        vec![block(LayoutStyle::new().height(48.0))],
    )
    .unwrap();
    let column = Container::new(
        LayoutStyle::new().flex_column().gap(10.0),
        vec![
            block(LayoutStyle::new().height(100.0)),
            Box::new(layer),
            after,
        ],
    )
    .unwrap();
    compute_layout(
        column.layout_node(),
        AvailableSpace::Definite(SURFACE.width),
        AvailableSpace::Definite(SURFACE.height),
    )
    .unwrap();
    assert_eq!(track_layout(after_node).unwrap().get().y, 110.0);
}

#[test]
fn the_page_scrolling_under_it_never_moves_it() {
    let (page, bar) = page_with_layer(Rc::default());
    page.viewport().scroll_to(0.0, 500.0);
    relayout_if_dirty();
    assert_eq!(page.viewport().peek_offset(), (0.0, 500.0));
    assert_eq!(
        track_layout(bar).unwrap().get(),
        Rect::new(0.0, 0.0, 800.0, 48.0)
    );
    assert_eq!(
        visible_rect(bar),
        Some(Rect::new(0.0, 0.0, 800.0, 48.0)),
        "and it is found where it is drawn, not where the page has scrolled to"
    );
}

#[test]
fn it_takes_the_pointer_over_its_own_boxes_and_nowhere_else() {
    let taps = Rc::new(Cell::new(0));
    let (page, _bar) = page_with_layer(taps.clone());
    page.viewport().scroll_to(0.0, 500.0);

    assert_eq!(dispatch_overlays(&press(400.0, 20.0)), EventResult::Handled);
    assert_eq!(
        dispatch_overlays(&release(400.0, 20.0)),
        EventResult::Handled
    );
    assert_eq!(taps.get(), 1, "a tap on the bar is the bar's");

    assert_eq!(
        dispatch_overlays(&press(400.0, 300.0)),
        EventResult::Ignored,
        "below the bar the press goes on to the page, though the layer spans the surface"
    );
    dispatch_overlays(&release(400.0, 300.0));
    assert_eq!(taps.get(), 1);
}

#[test]
fn its_focusables_keep_their_place_in_the_tab_order_and_do_not_trap_it() {
    reset_layout_runtime();
    set_surface_size(SURFACE);
    let control = || {
        StyledContainer::new(
            LayoutStyle::new().width(50.0).height(20.0),
            |_r| renderer_core::RectStyle::default(),
            vec![],
        )
        .unwrap()
        .on_focus(|_| {})
    };
    let inside = control();
    let inside_id = inside.focus_id();
    let layer = FixedLayer::new(LayoutStyle::new(), vec![Box::new(inside)]).unwrap();
    let (before, after) = (control(), control());
    let (before_id, after_id) = (before.focus_id(), after.focus_id());
    let _page = Container::new(
        LayoutStyle::new().flex_column(),
        vec![Box::new(before), Box::new(layer), Box::new(after)],
    )
    .unwrap();

    focus::request(before_id.unwrap());
    focus::focus_next();
    assert_eq!(
        focus::current(),
        inside_id,
        "Tab goes from what comes before the layer into it, though the layer's content registered first"
    );
    focus::focus_next();
    assert_eq!(
        focus::current(),
        after_id,
        "and out of it to what comes after"
    );
}

fn opened_at(commands: &[DrawCommand], node: NodeId) -> usize {
    let id = ElementId(node.into());
    commands
        .iter()
        .position(
            |command| matches!(command, DrawCommand::PushElement { element } if element.id == id),
        )
        .expect("the box is in the frame")
}

#[test]
fn it_is_drawn_over_the_sticky_boxes_the_page_declares_after_it() {
    reset_layout_runtime();
    set_surface_size(SURFACE);
    let bar = bar(Rc::default());
    let bar_node = bar.layout_node();
    let stage = block(LayoutStyle::new().sticky().inset_top(0.0).height(600.0));
    let stage_node = stage.layout_node();
    let mut page = ScrollPage::new_with(|_| {
        let layer = FixedLayer::new(LayoutStyle::new().flex_column(), vec![Box::new(bar)])?;
        let content = Container::new(
            LayoutStyle::new().flex_column(),
            vec![
                Box::new(layer),
                Box::new(Container::new(LayoutStyle::new().height(1800.0), vec![stage]).unwrap()),
            ],
        )?;
        Ok(Box::new(content))
    })
    .unwrap();
    page.relayout(SURFACE.width, SURFACE.height);
    relayout_if_dirty();

    let root = SegmentRoot::mount(page);
    let commands = root.commands();
    assert!(opened_at(&commands, bar_node) > opened_at(&commands, stage_node));
}

/// A document hides the layer with the box it was declared in, since the layer's element stands in that box; every other target draws its content as a root of its own, so it has to be told.
#[test]
fn a_box_hiding_where_the_layer_was_declared_hides_the_layer() {
    reset_layout_runtime();
    set_surface_size(SURFACE);
    let bar = bar(Rc::default());
    let bar_node = bar.layout_node();
    let layer = FixedLayer::new(LayoutStyle::new().flex_column(), vec![Box::new(bar)]).unwrap();
    let section = Container::new(LayoutStyle::new().flex_column(), vec![Box::new(layer)]).unwrap();
    let section_node = section.layout_node();
    let root = SegmentRoot::mount(section);
    let drawn = |root: &SegmentRoot| {
        let id = ElementId(bar_node.into());
        root.commands().iter().any(
            |command| matches!(command, DrawCommand::PushElement { element } if element.id == id),
        )
    };
    assert!(drawn(&root));

    crate::context::set_layout_style(section_node, LayoutStyle::new().flex_column().shown(false))
        .unwrap();
    assert!(!drawn(&root), "hidden with the section it was declared in");

    crate::context::set_layout_style(section_node, LayoutStyle::new().flex_column().shown(true))
        .unwrap();
    assert!(drawn(&root), "and back when the section is");
}

/// A switch in a layer is a control like any other: Tab reaches it where the layer was declared, the keyboard flips it through the layer, and it says the state it is in.
#[test]
fn a_switch_in_a_layer_is_reached_by_tab_and_flipped_by_the_keyboard() {
    reset_layout_runtime();
    set_surface_size(SURFACE);
    let before = focus::next_id();
    focus::register_as(before, focus::FocusKind::Widget);

    let on = reactive_core::signal(false);
    let below = focus::next_id();
    let switch = StyledContainer::new(
        LayoutStyle::new().width(50.0).height(20.0),
        |_r| renderer_core::RectStyle::default(),
        vec![],
    )
    .unwrap()
    .control(focus::Role::Switch)
    .toggled(move || on.get())
    .on_press(move || on.set(!on.peek()));
    let mut layer = FixedLayer::new(LayoutStyle::new(), vec![Box::new(switch)]).unwrap();
    let above = focus::next_id();
    relayout_if_dirty();

    focus::request(before);
    focus::focus_next();
    let landed = focus::current().expect("something took focus");
    assert!(
        landed > below && landed < above,
        "Tab goes from what comes before the layer to its switch"
    );

    let key = |named| Event::KeyPressed {
        key: platform_core::Key::Named(named),
        modifiers: platform_core::ModifiersState::default(),
        unmodified: None,
    };
    assert_eq!(
        layer.on_event(&key(platform_core::NamedKey::Space)),
        EventResult::Handled
    );
    assert!(on.peek(), "Space flips it");
    assert_eq!(
        layer.on_event(&key(platform_core::NamedKey::Enter)),
        EventResult::Handled
    );
    assert!(!on.peek(), "and so does Enter");

    on.set(true);
    let switch = focus::exposed()
        .into_iter()
        .find(|exposed| exposed.id == landed)
        .expect("the switch is exposed");
    assert_eq!(switch.role, focus::Role::Switch);
    assert_eq!(switch.toggled, Some(true));
}

/// A box in a layer scaled from its start grows where its origin says, in the surface's coordinates the layer lays it out in, however far the page under it has scrolled; the layer takes the pointer over what it draws, not over the box it laid out.
#[test]
fn a_box_in_a_layer_scaled_from_its_start_takes_the_pointer_where_it_is_drawn() {
    use crate::{TransformOrigin, box_transform_about};

    let taps = Rc::new(Cell::new(0));
    reset_layout_runtime();
    set_surface_size(SURFACE);
    let sink = taps.clone();
    let badge = StyledContainer::new(
        LayoutStyle::new().width(100.0).height(48.0),
        |_r| renderer_core::RectStyle::default(),
        vec![],
    )
    .unwrap()
    .with_transform(|r| box_transform_about(r, TransformOrigin::start(), 0.0, 2.0, 2.0, 0.0, 0.0))
    .on_press(move || sink.set(sink.get() + 1));
    let badge_node = badge.layout_node();
    let mut page = ScrollPage::new_with(|_| {
        let layer = FixedLayer::new(
            LayoutStyle::new()
                .flex_column()
                .align_items(layout_core::AlignItems::START),
            vec![Box::new(badge)],
        )?;
        let content = Container::new(
            LayoutStyle::new().flex_column(),
            vec![Box::new(layer), block(LayoutStyle::new().height(2000.0))],
        )?;
        Ok(Box::new(content))
    })
    .unwrap();
    page.relayout(SURFACE.width, SURFACE.height);
    relayout_if_dirty();
    page.viewport().scroll_to(0.0, 500.0);
    relayout_if_dirty();
    assert_eq!(
        track_layout(badge_node).unwrap().get(),
        Rect::new(0.0, 0.0, 100.0, 48.0)
    );

    let tap = |x, y| {
        let pressed = dispatch_overlays(&press(x, y));
        dispatch_overlays(&release(x, y));
        pressed
    };
    assert_eq!(tap(150.0, 20.0), EventResult::Handled);
    assert_eq!(
        taps.get(),
        1,
        "the growth past its laid-out edge is the box's"
    );
    assert_eq!(
        tap(250.0, 20.0),
        EventResult::Ignored,
        "past what it draws the press goes on to the page"
    );
    assert_eq!(taps.get(), 1);
}

fn moved(x: f64, y: f64) -> Event {
    Event::PointerMoved {
        x,
        y,
        source: PointerSource::Mouse,
    }
}

#[test]
fn a_box_in_a_layer_stops_being_hovered_once_the_pointer_moves_off_the_layer() {
    reset_layout_runtime();
    set_surface_size(SURFACE);
    let hovered = Rc::new(Cell::new(false));
    let bar = StyledContainer::new(
        LayoutStyle::new()
            .width(SizeDimension::Percent(1.0))
            .height(48.0),
        |_| renderer_core::RectStyle::default(),
        vec![],
    )
    .unwrap()
    .on_hover({
        let hovered = hovered.clone();
        move |now| hovered.set(now)
    });
    let _layer = FixedLayer::new(LayoutStyle::new().flex_column(), vec![Box::new(bar)]).unwrap();
    relayout_if_dirty();

    dispatch_overlays(&moved(400.0, 20.0));
    assert!(hovered.get());

    assert_eq!(
        dispatch_overlays(&moved(400.0, 300.0)),
        EventResult::Ignored,
        "the move below the bar is the page's"
    );
    assert!(!hovered.get(), "but the bar hears that the pointer left it");
}
