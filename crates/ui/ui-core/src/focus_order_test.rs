use layout_core::{AvailableSpace, LayoutStyle};
use platform_core::{Event, Key, ModifiersState, NamedKey};
use renderer_core::RectStyle;

use super::*;
use crate::container::Container;
use crate::context::{compute_layout, reset_layout_runtime};
use crate::layout_item::{LayoutItem, box_item};
use crate::scroll_area::LayoutScrollArea;
use crate::styled_container::StyledContainer;

fn control(height: f32) -> StyledContainer {
    StyledContainer::new(
        LayoutStyle::new().width(80.0).height(height),
        |_| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .control(Role::Button)
    .on_press(|| {})
}

fn block(height: f32) -> Box<dyn LayoutItem> {
    box_item(Container::new(LayoutStyle::new().width(80.0).height(height), vec![]).unwrap())
}

fn scroll(height: f32, content: Vec<Box<dyn LayoutItem>>) -> LayoutScrollArea {
    let content = Container::new(LayoutStyle::new().flex_column(), content).unwrap();
    LayoutScrollArea::new(
        LayoutStyle::new().width(200.0).height(height),
        box_item(content),
    )
    .unwrap()
}

fn page(children: Vec<Box<dyn LayoutItem>>) -> Container {
    let page = Container::new(LayoutStyle::new().flex_column(), children).unwrap();
    compute_layout(
        page.layout_node(),
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(800.0),
    )
    .unwrap();
    page
}

fn walk(steps: usize) -> Vec<Option<FocusId>> {
    clear();
    (0..steps)
        .map(|_| {
            focus_next();
            current()
        })
        .collect()
}

fn press(key: NamedKey) -> Event {
    Event::KeyPressed {
        key: Key::Named(key),
        modifiers: ModifiersState::default(),
    }
}

/// Tab follows the page, not the order its parts were built in: a list built before the search field drawn above it — as a sidebar built from its body up does — is still reached after the field.
#[test]
fn tab_walks_the_controls_in_document_order_whatever_order_they_were_built_in() {
    reset_layout_runtime();
    clear();
    let list = control(30.0);
    let field = control(30.0);
    let (list_id, field_id) = (list.focus_id(), field.focus_id());
    let _page = page(vec![box_item(field), box_item(list)]);

    assert_eq!(walk(3), vec![field_id, list_id, field_id]);
}

/// A scroll's content is laid out as a root of its own, so it has no place in the page by layout alone: it reads where the scroll that shows it is declared.
#[test]
fn a_control_in_a_scroll_area_is_reached_where_the_scroll_area_stands() {
    reset_layout_runtime();
    clear();
    let inner = control(30.0);
    let inner_id = inner.focus_id();
    let area = scroll(100.0, vec![box_item(inner)]);
    let (before, after) = (control(30.0), control(30.0));
    let (before_id, after_id) = (before.focus_id(), after.focus_id());
    let _page = page(vec![box_item(before), box_item(area), box_item(after)]);

    assert_eq!(walk(3), vec![before_id, inner_id, after_id]);
}

/// A region the keyboard can only scroll — text and nothing to focus — is a stop of its own, or no key could ever move it.
#[test]
fn a_scroll_area_holding_only_text_is_a_stop_and_its_keys_scroll_it() {
    reset_layout_runtime();
    clear();
    let area = scroll(100.0, vec![block(1000.0)]);
    let content = area.content_node();
    let (before, after) = (control(30.0), control(30.0));
    let (before_id, after_id) = (before.focus_id(), after.focus_id());
    let mut tree = ui_tree::ComponentList::new(page(vec![
        box_item(before),
        box_item(area),
        box_item(after),
    ]));

    let stops = walk(3);
    assert_eq!(stops[0], before_id);
    assert_eq!(stops[2], after_id);
    let area_id = stops[1].expect("the scroll area is a stop between them");
    assert!(
        exposed()
            .iter()
            .any(|e| e.id == area_id && e.role == Role::ScrollArea),
        "and a reader is told what holds focus"
    );

    let viewports = crate::scroll_viewports::enclosing_scroll_viewport(content)
        .expect("the content has a viewport");
    let ringed = |tree: &ui_tree::ComponentList| {
        let view = viewports.rect().peek();
        tree.commands().iter().any(|command| {
            matches!(command, renderer_core::DrawCommand::Rect { rect, style } if *rect == view && style.border.is_some())
        })
    };
    assert!(!ringed(&tree));
    request(area_id);
    assert!(
        ringed(&tree),
        "the keyboard reached it, so it shows where the keys go"
    );
    tree.on_event(&press(NamedKey::ArrowDown));
    let line = platform_core::ScrollDelta::Lines { x: 0.0, y: 1.0 }
        .pixels()
        .1;
    assert_eq!(viewports.peek_offset().1, line, "an arrow scrolls a line");
    tree.on_event(&press(NamedKey::End));
    assert_eq!(viewports.peek_offset().1, 900.0, "End goes to the bottom");
    tree.on_event(&press(NamedKey::PageUp));
    assert_eq!(
        viewports.peek_offset().1,
        900.0 - (100.0 - line),
        "a page is the view less a line"
    );
    tree.on_event(&press(NamedKey::Home));
    assert_eq!(viewports.peek_offset().1, 0.0);
    clear();
}

/// A region that holds controls is scrolled by moving through them, so it adds no stop of its own between the field above it and its first control.
#[test]
fn a_scroll_area_holding_controls_adds_no_stop() {
    reset_layout_runtime();
    clear();
    let inner = control(30.0);
    let inner_id = inner.focus_id();
    let area = scroll(100.0, vec![block(1000.0), box_item(inner)]);
    let field = control(30.0);
    let field_id = field.focus_id();
    let _page = page(vec![box_item(field), box_item(area)]);

    assert_eq!(walk(3), vec![field_id, inner_id, field_id]);
    assert!(
        exposed().iter().all(|e| e.role != Role::ScrollArea),
        "nor is it announced as a control"
    );
}

/// What fits has nothing to scroll.
#[test]
fn a_scroll_area_whose_content_fits_is_no_stop() {
    reset_layout_runtime();
    clear();
    let area = scroll(100.0, vec![block(40.0)]);
    let field = control(30.0);
    let field_id = field.focus_id();
    let _page = page(vec![box_item(field), box_item(area)]);

    assert_eq!(walk(2), vec![field_id, field_id]);
}

/// The page's own scroll is the window's: it scrolls with nothing focused, and a stop for it would be a stop for the whole window.
#[test]
fn the_page_scroll_is_no_stop() {
    reset_layout_runtime();
    clear();
    let mut page = crate::ScrollPage::new(block(2000.0)).unwrap();
    page.relayout(400.0, 800.0);

    assert_eq!(walk(1), vec![None]);
}
