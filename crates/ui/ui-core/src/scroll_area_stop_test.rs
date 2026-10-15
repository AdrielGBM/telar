use std::sync::Arc;

use layout_core::{AvailableSpace, LayoutStyle, NodeId};
use platform_core::{
    ConsumedKeys, Event, Key, ModifiersState, NamedKey, PointerButton, PointerSource,
};
use reactive_core::{RwSignal, signal};
use renderer_core::{DrawCommand, RectStyle, Role};

use super::*;
use crate::container::Container;
use crate::context::{compute_layout, reset_layout_runtime};
use crate::layout_item::{LayoutItem, box_item};
use crate::styled_container::{StyledContainer, style_follows};

const VIEW: (f32, f32) = (200.0, 100.0);

struct AsADocument(bool);

impl AsADocument {
    fn new() -> Self {
        Self(ui_tree::set_element_capture(true))
    }
}

impl Drop for AsADocument {
    fn drop(&mut self) {
        ui_tree::set_element_capture(self.0);
    }
}

fn block(width: f32, height: f32) -> Box<dyn LayoutItem> {
    box_item(Container::new(LayoutStyle::new().width(width).height(height), vec![]).unwrap())
}

fn button(height: f32) -> StyledContainer {
    StyledContainer::new(
        LayoutStyle::new().width(80.0).height(height),
        |_| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .control(Role::Button)
    .on_press(|| {})
}

fn column(children: Vec<Box<dyn LayoutItem>>) -> Container {
    Container::new(LayoutStyle::new().flex_column(), children).unwrap()
}

fn area(content: Container) -> (LayoutScrollArea, ScrollViewport) {
    let mut viewport = None;
    let area =
        LayoutScrollArea::new_with(LayoutStyle::new().width(VIEW.0).height(VIEW.1), |handed| {
            viewport = Some(handed);
            Ok(box_item(content))
        })
        .unwrap();
    (area, viewport.unwrap())
}

fn mounted(area: LayoutScrollArea) -> (ui_tree::ComponentList, NodeId) {
    let node = area.layout_node();
    lay_out(node);
    (ui_tree::ComponentList::new(area), node)
}

fn lay_out(node: NodeId) {
    compute_layout(
        node,
        AvailableSpace::Definite(VIEW.0),
        AvailableSpace::Definite(VIEW.1),
    )
    .unwrap();
}

fn element(tree: &ui_tree::ComponentList, node: NodeId) -> Arc<renderer_core::Element> {
    let id = renderer_core::ElementId(node.into());
    tree.commands()
        .iter()
        .find_map(|command| match command {
            DrawCommand::PushElement { element } if element.id == id => Some(element.clone()),
            _ => None,
        })
        .expect("the area is an element")
}

fn the_stop(tree: &ui_tree::ComponentList, node: NodeId) -> Option<renderer_core::Focusable> {
    element(tree, node).semantics.focusable
}

fn stops() -> Vec<Role> {
    focus::tab_order().iter().map(|stop| stop.role).collect()
}

fn key(key: NamedKey, modifiers: ModifiersState) -> Event {
    Event::KeyPressed {
        key: Key::Named(key),
        modifiers,
        unmodified: None,
    }
}

fn press(key_named: NamedKey) -> Event {
    key(key_named, ModifiersState::default())
}

fn pressed_at(x: f64, y: f64) -> Event {
    Event::PointerPressed {
        x,
        y,
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    }
}

fn ringed(tree: &ui_tree::ComponentList) -> bool {
    let view = Rect::new(0.0, 0.0, VIEW.0, VIEW.1);
    tree.commands().iter().any(|command| {
        matches!(command, DrawCommand::Rect { rect, style } if *rect == view && style.border.is_some())
    })
}

fn line() -> f32 {
    ScrollDelta::Lines { x: 0.0, y: 1.0 }.pixels().1
}

fn sized_by(height: RwSignal<f32>) -> (Container, reactive_core::Effect) {
    let tall = Container::new(LayoutStyle::new(), vec![]).unwrap();
    let follows = style_follows(tall.layout_node(), move || {
        LayoutStyle::new().width(80.0).height(height.get())
    });
    (column(vec![box_item(tall)]), follows)
}

#[test]
fn an_area_that_overflows_downwards_with_nothing_to_focus_is_a_stop_keeping_the_keys_that_scroll_it()
 {
    reset_layout_runtime();
    focus::clear();
    let _document = AsADocument::new();
    let (area, _) = area(column(vec![block(80.0, 1000.0)]));
    let (tree, node) = mounted(area);

    let stop = the_stop(&tree, node).expect("the document can focus it");
    assert!(stop.tab_stop, "Tab stops on it");
    assert_eq!(
        stop.consumes,
        ConsumedKeys::VERTICAL_ARROWS
            | ConsumedKeys::PAGING
            | ConsumedKeys::EDGES
            | ConsumedKeys::SPACE,
        "the side arrows scroll nothing here, so they are left to the host"
    );
    assert_eq!(element(&tree, node).semantics.role, Role::ScrollArea);
    assert_eq!(stops(), vec![Role::ScrollArea]);
    assert!(
        focus::exposed().iter().any(|e| e.role == Role::ScrollArea),
        "a reader is told about it"
    );
}

#[test]
fn an_area_that_fits_or_holds_a_stop_is_none_of_its_own() {
    reset_layout_runtime();
    focus::clear();
    let _document = AsADocument::new();
    let (fits, _) = area(column(vec![block(80.0, 40.0)]));
    let (tree, node) = mounted(fits);
    assert_eq!(the_stop(&tree, node), None);
    assert!(stops().is_empty());

    reset_layout_runtime();
    let (holds, _) = area(column(vec![block(80.0, 1000.0), box_item(button(30.0))]));
    let (tree, node) = mounted(holds);
    assert_eq!(the_stop(&tree, node), None);
    assert_eq!(stops(), vec![Role::Button]);
}

/// Rows a cursor walks while focus stays on the control driving them are no Tab stops, so they leave a region nothing else reaches to a stop of its own.
#[test]
fn rows_that_are_only_presented_leave_the_area_a_stop() {
    reset_layout_runtime();
    focus::clear();
    let row = StyledContainer::new(
        LayoutStyle::new().width(80.0).height(1000.0),
        |_| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .presented(Role::Option);
    let (area, _) = area(column(vec![box_item(row)]));
    let _tree = mounted(area);

    assert_eq!(stops(), vec![Role::ScrollArea]);
}

#[test]
fn keys_scroll_a_focused_area_as_they_scroll_a_page() {
    reset_layout_runtime();
    focus::clear();
    let (area, viewport) = area(column(vec![block(80.0, 1000.0)]));
    let (mut tree, _) = mounted(area);
    focus::focus_next();
    let shift = ModifiersState {
        is_shift: true,
        ..ModifiersState::default()
    };
    let page = VIEW.1 - line();

    tree.on_event(&press(NamedKey::ArrowDown));
    assert_eq!(viewport.peek_offset().1, line());
    tree.on_event(&press(NamedKey::ArrowUp));
    assert_eq!(viewport.peek_offset().1, 0.0);
    tree.on_event(&press(NamedKey::Space));
    assert_eq!(viewport.peek_offset().1, page, "Space pages down");
    tree.on_event(&key(NamedKey::Space, shift));
    assert_eq!(viewport.peek_offset().1, 0.0, "Shift+Space pages up");
    tree.on_event(&press(NamedKey::PageDown));
    assert_eq!(viewport.peek_offset().1, page);
    tree.on_event(&press(NamedKey::End));
    assert_eq!(viewport.peek_offset().1, 900.0);
    tree.on_event(&press(NamedKey::Home));
    assert_eq!(viewport.peek_offset().1, 0.0);

    let ctrl = ModifiersState {
        is_ctrl: true,
        ..ModifiersState::default()
    };
    assert_eq!(
        tree.on_event(&key(NamedKey::ArrowDown, ctrl)),
        EventResult::Ignored,
        "a chord is a shortcut's, not the scroll's"
    );
    assert_eq!(
        tree.on_event(&press(NamedKey::ArrowRight)),
        EventResult::Ignored,
        "nothing to scroll across"
    );
    assert_eq!(viewport.peek_offset(), (0.0, 0.0));
}

#[test]
fn an_area_that_overflows_across_scrolls_with_the_side_arrows() {
    reset_layout_runtime();
    focus::clear();
    let _document = AsADocument::new();
    let wide = Container::new(LayoutStyle::new().width(1000.0).height(40.0), vec![]).unwrap();
    let (area, viewport) = area(wide);
    let (mut tree, node) = mounted(area);

    assert_eq!(
        the_stop(&tree, node).map(|stop| stop.consumes),
        Some(ConsumedKeys::HORIZONTAL_ARROWS)
    );
    focus::focus_next();
    tree.on_event(&press(NamedKey::ArrowRight));
    assert_eq!(viewport.peek_offset(), (line(), 0.0));
    tree.on_event(&press(NamedKey::ArrowLeft));
    assert_eq!(viewport.peek_offset(), (0.0, 0.0));
    assert_eq!(
        tree.on_event(&press(NamedKey::PageDown)),
        EventResult::Ignored
    );
}

#[test]
fn the_keyboard_rings_it_and_a_press_focuses_it_without_a_ring() {
    reset_layout_runtime();
    focus::clear();
    let (area, _) = area(column(vec![block(80.0, 1000.0)]));
    let (mut tree, _) = mounted(area);
    assert!(!ringed(&tree));

    focus::focus_next();
    let id = focus::current().expect("Tab reached it");
    assert!(ringed(&tree), "the keyboard reached it, so it shows");

    focus::clear();
    focus::blur_from_pointer(20.0, 20.0);
    tree.on_event(&pressed_at(20.0, 20.0));
    assert_eq!(focus::current(), Some(id), "a press inside focuses it");
    assert!(!ringed(&tree), "as a tap focuses, without a ring");
}

#[test]
fn a_press_in_an_area_holding_a_stop_does_not_focus_it() {
    reset_layout_runtime();
    focus::clear();
    let (area, _) = area(column(vec![block(80.0, 1000.0), box_item(button(30.0))]));
    let (mut tree, _) = mounted(area);

    tree.on_event(&pressed_at(20.0, 20.0));
    assert_eq!(focus::current(), None);
}

#[test]
fn content_shrinking_to_fit_ends_the_stop_and_growing_back_brings_it_back() {
    reset_layout_runtime();
    focus::clear();
    let _document = AsADocument::new();
    let height = signal(1000.0);
    let (content, _follows) = sized_by(height);
    let (area, _) = area(content);
    let (tree, node) = mounted(area);
    assert!(the_stop(&tree, node).is_some_and(|stop| stop.tab_stop));

    height.set(40.0);
    lay_out(node);
    assert_eq!(the_stop(&tree, node), None);
    assert!(stops().is_empty());

    height.set(1000.0);
    lay_out(node);
    assert!(the_stop(&tree, node).is_some_and(|stop| stop.tab_stop));
    assert_eq!(stops(), vec![Role::ScrollArea]);
}

#[test]
fn a_control_showing_inside_ends_the_stop() {
    reset_layout_runtime();
    focus::clear();
    let _document = AsADocument::new();
    let shown = signal(false);
    let inside = button(30.0);
    let _follows = style_follows(inside.layout_node(), move || match shown.get() {
        true => LayoutStyle::new().width(80.0).height(30.0),
        false => LayoutStyle::new().display_none(),
    });
    let (area, _) = area(column(vec![block(80.0, 1000.0), box_item(inside)]));
    let (tree, node) = mounted(area);
    assert!(the_stop(&tree, node).is_some_and(|stop| stop.tab_stop));

    shown.set(true);
    lay_out(node);
    assert_eq!(the_stop(&tree, node), None);
    assert_eq!(stops(), vec![Role::Button]);
}

#[test]
fn a_control_built_into_the_content_later_ends_the_stop() {
    reset_layout_runtime();
    focus::clear();
    let _document = AsADocument::new();
    let content = column(vec![block(80.0, 1000.0)]);
    let content_node = content.layout_node();
    let (area, _) = area(content);
    let (tree, node) = mounted(area);
    assert!(the_stop(&tree, node).is_some_and(|stop| stop.tab_stop));

    let late = button(30.0);
    let mut children = layout_reactive::children(content_node);
    children.push(late.layout_node());
    crate::context::set_children(content_node, &children).unwrap();
    lay_out(node);
    assert_eq!(the_stop(&tree, node), None);

    drop(late);
    assert!(
        the_stop(&tree, node).is_some_and(|stop| stop.tab_stop),
        "and it is a stop again once the control is gone"
    );
}

/// A stop that disappeared from under the keyboard would leave Tab with nowhere to go on from.
#[test]
fn focus_on_the_area_keeps_it_a_stop_until_focus_moves_on() {
    reset_layout_runtime();
    focus::clear();
    let _document = AsADocument::new();
    let height = signal(1000.0);
    let (content, _follows) = sized_by(height);
    let (area, _) = area(content);
    let (tree, node) = mounted(area);
    focus::focus_next();
    let id = focus::current().expect("Tab reached it");

    height.set(40.0);
    lay_out(node);
    assert_eq!(focus::current(), Some(id), "focus stays where it was");
    assert!(the_stop(&tree, node).is_some_and(|stop| stop.tab_stop));
    assert_eq!(stops(), vec![Role::ScrollArea]);

    focus::clear();
    assert_eq!(the_stop(&tree, node), None);
    assert!(stops().is_empty());
}

#[test]
fn a_primary_scroll_is_never_a_stop() {
    reset_layout_runtime();
    focus::clear();
    let _document = AsADocument::new();
    let (area, _) = area(column(vec![block(80.0, 1000.0)]));
    let (tree, node) = mounted(area.into_primary());

    assert_eq!(the_stop(&tree, node), None);
    assert!(stops().is_empty());
}

fn wide_row() -> Container {
    Container::new(
        LayoutStyle::new().flex_row(),
        vec![box_item(
            Container::new(
                LayoutStyle::new()
                    .width(1000.0)
                    .height(40.0)
                    .flex_shrink(0.0),
                vec![],
            )
            .unwrap(),
        )],
    )
    .unwrap()
}

#[test]
fn a_row_running_past_the_area_overflows_across_and_is_a_stop() {
    reset_layout_runtime();
    focus::clear();
    let _document = AsADocument::new();
    let (area, viewport) = area(wide_row());
    let (mut tree, node) = mounted(area);

    assert_eq!(
        the_stop(&tree, node).map(|stop| stop.consumes),
        Some(ConsumedKeys::HORIZONTAL_ARROWS)
    );
    assert_eq!(stops(), vec![Role::ScrollArea]);
    focus::focus_next();
    tree.on_event(&press(NamedKey::ArrowRight));
    assert_eq!(viewport.peek_offset(), (line(), 0.0));
}

#[test]
fn the_wheel_scrolls_a_row_running_past_the_area_to_its_end() {
    reset_layout_runtime();
    let (area, viewport) = area(wide_row());
    let (mut tree, _) = mounted(area);
    tree.on_event(&Event::Scrolled {
        delta: ScrollDelta::Pixels { x: -5000.0, y: 0.0 },
        x: 50.0,
        y: 50.0,
    });
    assert_eq!(viewport.peek_offset(), (1000.0 - VIEW.0, 0.0));
}

#[test]
fn a_row_running_past_the_area_draws_the_horizontal_scrollbar() {
    reset_layout_runtime();
    let (area, _) = area(wide_row());
    let (tree, _) = mounted(area);
    let bar = tree.commands().iter().any(|command| {
        matches!(command, DrawCommand::Rect { rect, .. } if rect.width < VIEW.0 && rect.height < 12.0 && rect.y > VIEW.1 - 12.0)
    });
    assert!(bar, "a thumb along the bottom edge");
}
