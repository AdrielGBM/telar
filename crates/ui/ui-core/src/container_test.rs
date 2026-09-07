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

/// The first half of the bug `crate::context` describes: a declaration left behind lands on whatever the next tree builds on that id.
///
/// Note what this does *not* call. `crate::context::reset_layout_runtime` sweeps the cascade too, and that pairing is the workaround — the reason the two "have to go together". Here only the layout half runs, so what keeps the second tree clean is the owner having withdrawn, not the reset.
///
/// It also has to replace the runtime rather than merely drop and rebuild. Taffy's `NodeId` packs a slotmap version, so within one runtime a recycled slot yields a *different* id and the collision cannot happen. A fresh `LayoutRuntime` starts its versions over, which is what hands the second tree the first tree's ids exactly.
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
/// Nothing about a late `Drop` needs a recycled id to be wrong — it only needs the declaration on that id to be somebody else's by then. Under the owner it withdraws nothing, because withdrawal is not its job any more.
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
    assert!(result.is_ok(), "a row container builds");
}

#[test]
fn container_column_creates_ok() {
    reset_layout_runtime();
    let result = Container::column(vec![]);
    assert!(result.is_ok(), "a column container builds");
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
    assert!(
        matches!(result, EventResult::Ignored),
        "a container with no handlers is not the one to answer"
    );
}

#[test]
fn container_layout_node_is_valid() {
    reset_layout_runtime();
    let container = Container::new(LayoutStyle::new().flex_row(), vec![]).unwrap();
    let node = container.layout_node();
    let _root = new_container(LayoutStyle::new().flex_row(), &[node]).expect("should register");
}

#[test]
fn a_click_that_rerenders_the_widget_it_landed_on_does_not_panic() {
    use crate::context::track_layout;
    use crate::styled_container::StyledContainer;
    use platform_core::PointerButton;
    use reactive_core::{begin_batch, end_batch, signal};

    reset_layout_runtime();
    let s = signal(0i32);
    let s_cb = s;
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

    assert_eq!(s.get(), 1, "click should have incremented the signal");
}

// A pressable plain Container must publish its rect to the same registry `StyledContainer` uses, or a click-through surface never carves an input region for it.
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
    assert!(outer.is_ok(), "a container nests inside another");
}
