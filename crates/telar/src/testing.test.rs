use std::cell::Cell;
use std::rc::Rc;

use layout_core::LayoutStyle;
use renderer_core::RectStyle;
use ui_core::{FixedLayer, StyledContainer, WindowRoot, box_item};

use super::*;

fn plain(style: LayoutStyle, children: Vec<Box<dyn ui_core::LayoutItem>>) -> StyledContainer {
    StyledContainer::new(style, |_| RectStyle::default(), children).expect("a box builds")
}

/// A page box the whole surface tall, with a bar fixed over its top 40 pixels.
fn page_under_a_bar(hovered: &Rc<Cell<bool>>) -> Scoped<ComponentList> {
    ui_core::reset_layout_runtime();
    let hovered = Rc::clone(hovered);
    let tree = mount(
        move || {
            let page = plain(LayoutStyle::new().width(200.0).height(200.0), vec![])
                .on_hover(move |now| hovered.set(now));
            let bar = plain(LayoutStyle::new().width(200.0).height(40.0), vec![]);
            let layer = FixedLayer::new(LayoutStyle::new().flex_column(), vec![box_item(bar)])
                .expect("a layer builds");
            let root = plain(
                LayoutStyle::new().flex_column().width(200.0).height(200.0),
                vec![box_item(page), box_item(layer)],
            );
            WindowRoot::new(box_item(root))
        },
        200,
        200,
    );
    crate::relayout_if_dirty();
    tree
}

#[test]
fn a_page_box_hears_the_pointer_leave_it_for_a_bar_fixed_over_it() {
    let hovered = Rc::new(Cell::new(false));
    let mut tree = page_under_a_bar(&hovered);

    route(&mut tree, &moved(100.0, 120.0));
    assert!(
        hovered.get(),
        "precondition: the page box is under the pointer"
    );

    route(&mut tree, &moved(100.0, 20.0));
    assert!(
        !hovered.get(),
        "the bar took the move, and the page box under it heard the pointer go"
    );

    route(&mut tree, &moved(100.0, 120.0));
    assert!(hovered.get(), "and hears it come back");
}

#[test]
fn dropping_what_scoped_built_stops_the_effects_it_started() {
    let source = crate::signal(0u32);
    let runs = Rc::new(Cell::new(0u32));
    let built = scoped(|| {
        let runs = Rc::clone(&runs);
        crate::effect(move || {
            source.get();
            runs.set(runs.get() + 1);
        })
    });
    source.set(1);
    assert_eq!(runs.get(), 2, "precondition: the effect follows its source");

    drop(built);
    source.set(2);
    assert_eq!(runs.get(), 2);
}
