use std::cell::{Cell, RefCell};
use std::rc::Rc;

use layout_core::AvailableSpace;
use platform_core::{PointerButton, PointerSource};
use reactive_core::effect;
use renderer_core::RectStyle;

use super::*;
use crate::container::Container;
use crate::context::{compute_layout, reset_layout_runtime};
use crate::dismiss::{DismissRegistration, dismiss_depth};
use crate::focus;
use crate::input_region::mentions;
use crate::layout_item::box_item;
use crate::overlay::Overlay;
use crate::reactive_list::ReactiveList;
use crate::styled_container::StyledContainer;

fn leaf() -> Result<Box<dyn LayoutItem>, LayoutError> {
    Ok(Box::new(Container::new(
        LayoutStyle::new().width(10.0).height(10.0),
        vec![],
    )?))
}

fn group_len(node: &RenderNode) -> usize {
    match node {
        RenderNode::Element { children, .. } => group_len(&children[0]),
        RenderNode::Group { children, .. } => children.len(),
        _ => panic!("expected Group"),
    }
}

fn counter() -> (Rc<Cell<u32>>, impl Fn()) {
    let count = Rc::new(Cell::new(0));
    let bump = {
        let count = Rc::clone(&count);
        move || count.set(count.get() + 1)
    };
    (count, bump)
}

fn counting_fallback(
    bump: impl Fn() + 'static,
) -> impl FnOnce(BuildFailure) -> Result<Box<dyn LayoutItem>, LayoutError> {
    move |_| {
        bump();
        leaf()
    }
}

/// A leaf that says when it is dropped, which for one held inside an overlay is when the overlay registry let go of it.
struct DropFlag {
    inner: Container,
    dropped: Rc<Cell<bool>>,
}

impl Drop for DropFlag {
    fn drop(&mut self) {
        self.dropped.set(true);
    }
}

impl LayoutItem for DropFlag {
    fn layout_node(&self) -> NodeId {
        self.inner.layout_node()
    }
}

impl Component for DropFlag {
    fn view(&self) -> RenderNode {
        self.inner.view()
    }
}

/// A leaf whose render reads `ticks`, so it re-renders on every change and logs what it saw.
struct Probe {
    inner: Container,
    ticks: RwSignal<i32>,
    seen: Rc<RefCell<Vec<i32>>>,
}

impl LayoutItem for Probe {
    fn layout_node(&self) -> NodeId {
        self.inner.layout_node()
    }
}

impl Component for Probe {
    fn view(&self) -> RenderNode {
        self.seen.borrow_mut().push(self.ticks.get());
        self.inner.view()
    }
}

#[test]
fn a_child_that_panics_mid_build_leaves_its_siblings_and_no_registrations() {
    reset_layout_runtime();
    focus::clear();
    let built: Rc<RefCell<Vec<NodeId>>> = Rc::default();
    let overlay_content_dropped = Rc::new(Cell::new(false));
    let failure: Rc<RefCell<Option<String>>> = Rc::default();

    let boundary = {
        let (built, dropped, failure) = (
            Rc::clone(&built),
            Rc::clone(&overlay_content_dropped),
            Rc::clone(&failure),
        );
        ErrorBoundary::new(
            move || {
                let button = StyledContainer::new(
                    LayoutStyle::new().width(20.0).height(20.0),
                    |_| RectStyle::default(),
                    vec![],
                )?
                .on_press(|| {})
                .control(focus::Role::Button);
                focus::focus_next();
                assert!(focus::current().is_some(), "the button took focus");
                let content = DropFlag {
                    inner: Container::new(LayoutStyle::new(), vec![])?,
                    dropped,
                };
                let overlay = Overlay::new(LayoutStyle::new(), vec![box_item(content)])?;
                built
                    .borrow_mut()
                    .extend([button.layout_node(), overlay.content_node()]);
                let held: Cell<Option<DismissRegistration>> = Cell::new(None);
                effect(move || held.set(Some(DismissRegistration::new(Rc::new(|| {})))));
                assert_eq!(dismiss_depth(), 1);
                assert!(built.borrow().iter().any(|&node| mentions(node)));
                let _still_held = (button, overlay);
                panic!("mid-build");
            },
            move |why| {
                *failure.borrow_mut() = Some(why.to_string());
                leaf()
            },
        )
        .unwrap()
    };

    assert!(boundary.has_failed());
    assert_eq!(
        failure.borrow().as_deref(),
        Some("build panicked: mid-build")
    );
    assert_eq!(group_len(&boundary.view()), 1, "the fallback is mounted");

    let row = Container::new(
        LayoutStyle::new().flex_row(),
        vec![leaf().unwrap(), box_item(boundary)],
    )
    .unwrap();
    assert_eq!(group_len(&row.view()), 2, "the sibling still renders");

    assert!(
        overlay_content_dropped.get(),
        "the overlay registry let go of its content"
    );
    for &node in built.borrow().iter() {
        assert!(!mentions(node), "the input region still names {node:?}");
    }
    assert_eq!(dismiss_depth(), 0, "the dismiss entry was withdrawn");
    assert_eq!(focus::current(), None, "focus left the failed subtree");
    focus::focus_next();
    assert_eq!(focus::current(), None, "nothing focusable is left behind");
}

#[test]
fn an_effect_that_panics_on_the_third_change_is_replaced_while_the_rest_keeps_updating() {
    reset_layout_runtime();
    let ticks = signal(0i32);
    let content_runs = Rc::new(Cell::new(0));
    let (fallbacks, bump) = counter();

    let boundary = {
        let runs = Rc::clone(&content_runs);
        ErrorBoundary::new(
            move || {
                effect(move || {
                    let _ = ticks.get();
                    runs.set(runs.get() + 1);
                    if runs.get() == 4 {
                        panic!("third change");
                    }
                });
                leaf()
            },
            counting_fallback(bump),
        )
        .unwrap()
    };
    let seen: Rc<RefCell<Vec<i32>>> = Rc::default();
    let _sibling = Container::new(
        LayoutStyle::new(),
        vec![box_item(Probe {
            inner: Container::new(LayoutStyle::new(), vec![]).unwrap(),
            ticks,
            seen: Rc::clone(&seen),
        })],
    )
    .unwrap();

    ticks.set(1);
    ticks.set(2);
    assert!(!boundary.has_failed());
    assert_eq!(fallbacks.get(), 0);

    ticks.set(3);
    assert!(boundary.has_failed());
    assert_eq!(fallbacks.get(), 1);
    assert_eq!(group_len(&boundary.view()), 1, "the fallback is mounted");

    ticks.set(4);
    ticks.set(5);
    assert_eq!(content_runs.get(), 4, "the failed subtree's effect is gone");
    assert_eq!(fallbacks.get(), 1, "the subtree fails over once");
    assert_eq!(
        *seen.borrow(),
        vec![0, 1, 2, 3, 4, 5],
        "the sibling kept up"
    );
}

#[test]
fn nested_boundaries_catch_at_the_innermost() {
    reset_layout_runtime();
    let ticks = signal(0i32);
    let (inner_failures, inner_bump) = counter();
    let inner_bump = Rc::new(inner_bump);
    let (outer_failures, outer_bump) = counter();

    let outer = ErrorBoundary::new(
        move || {
            let bump = Rc::clone(&inner_bump);
            let at_build =
                ErrorBoundary::new(|| panic!("inner build"), counting_fallback(move || bump()))?;
            let bump = Rc::clone(&inner_bump);
            let on_rerun = ErrorBoundary::new(
                move || {
                    effect(move || {
                        if ticks.get() == 1 {
                            panic!("inner re-run");
                        }
                    });
                    leaf()
                },
                counting_fallback(move || bump()),
            )?;
            Ok(box_item(Container::new(
                LayoutStyle::new(),
                vec![box_item(at_build), box_item(on_rerun)],
            )?))
        },
        counting_fallback(outer_bump),
    )
    .unwrap();
    assert_eq!(
        inner_failures.get(),
        1,
        "the inner build's panic stayed inner"
    );

    ticks.set(1);
    assert_eq!(inner_failures.get(), 2, "so did the inner re-run's");
    assert_eq!(outer_failures.get(), 0);
    assert!(!outer.has_failed());
}

#[test]
fn a_fallback_that_fails_hands_the_failure_to_the_boundary_above() {
    reset_layout_runtime();
    let ticks = signal(0i32);
    let (outer_failures, outer_bump) = counter();

    let outer = ErrorBoundary::new(
        move || {
            let inner = ErrorBoundary::new(
                move || {
                    effect(move || {
                        if ticks.get() == 1 {
                            panic!("content");
                        }
                    });
                    leaf()
                },
                |_| panic!("fallback too"),
            )?;
            Ok(box_item(inner))
        },
        counting_fallback(outer_bump),
    )
    .unwrap();

    ticks.set(1);
    assert!(outer.has_failed());
    assert_eq!(outer_failures.get(), 1);
}

#[test]
fn an_error_from_the_build_mounts_the_fallback_with_it() {
    reset_layout_runtime();
    let failure: Rc<RefCell<Option<BuildFailure>>> = Rc::default();
    let boundary = {
        let failure = Rc::clone(&failure);
        ErrorBoundary::new(
            || Err(LayoutError::Engine("no room".into())),
            move |why| {
                *failure.borrow_mut() = Some(why);
                leaf()
            },
        )
        .unwrap()
    };
    assert!(boundary.has_failed());
    assert!(matches!(
        failure.borrow().as_ref(),
        Some(BuildFailure::Failed(LayoutError::Engine(message))) if message == "no room"
    ));
}

/// A handler inside the subtree can be what fails it. The subtree is still executing that handler, so it is retired once the dispatch unwinds rather than freed under it.
#[test]
fn a_failure_raised_from_a_handler_inside_waits_for_the_dispatch() {
    reset_layout_runtime();
    let ticks = signal(0i32);
    let presses = Rc::new(Cell::new(0));
    let mut boundary = {
        let presses = Rc::clone(&presses);
        ErrorBoundary::new(
            move || {
                effect(move || {
                    if ticks.get() == 1 {
                        panic!("pressed");
                    }
                });
                Ok(box_item(
                    StyledContainer::new(
                        LayoutStyle::new().width(40.0).height(40.0),
                        |_| RectStyle::default(),
                        vec![],
                    )?
                    .on_press(move || {
                        ticks.set(1);
                        presses.set(presses.get() + 1);
                    }),
                ))
            },
            |_| leaf(),
        )
        .unwrap()
    };
    compute_layout(
        boundary.layout_node(),
        AvailableSpace::Definite(100.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();

    let at = |event: fn(f64, f64, PointerButton, PointerSource) -> Event| {
        event(20.0, 20.0, PointerButton::Primary, PointerSource::Mouse)
    };
    boundary.on_event(&at(|x, y, button, source| Event::PointerPressed {
        x,
        y,
        button,
        source,
    }));
    boundary.on_event(&at(|x, y, button, source| Event::PointerReleased {
        x,
        y,
        button,
        source,
    }));

    assert_eq!(presses.get(), 1, "the handler ran to its end");
    assert!(boundary.has_failed());
    ticks.set(2);
    assert_eq!(group_len(&boundary.view()), 1);
}

fn nested_box() -> Result<Box<dyn LayoutItem>, LayoutError> {
    Ok(Box::new(Container::new(
        LayoutStyle::new().flex_row(),
        vec![leaf()?, leaf()?],
    )?))
}

/// What a failed build made is freed with it, including nodes nothing had attached yet when it panicked.
#[test]
fn a_build_that_panics_leaves_no_layout_nodes_behind() {
    reset_layout_runtime();
    let baseline = crate::context::live_node_count();

    let boundary = ErrorBoundary::new(
        || {
            let _attached = Container::new(LayoutStyle::new(), vec![nested_box()?])?;
            let _loose = nested_box()?;
            panic!("mid-build");
        },
        |_| leaf(),
    )
    .unwrap();

    assert!(boundary.has_failed());
    assert_eq!(
        crate::context::live_node_count(),
        baseline + 2,
        "the boundary's node and the fallback's leaf"
    );
}

#[test]
fn a_rerun_that_panics_mid_row_leaves_no_layout_nodes_behind() {
    reset_layout_runtime();
    let items = signal(vec![1u32]);
    let baseline = crate::context::live_node_count();

    let boundary = ErrorBoundary::new(
        move || {
            Ok(box_item(ReactiveList::new(
                move || items.get(),
                |n: &u32| *n,
                |n| {
                    let row = nested_box()?;
                    if n == 3 {
                        panic!("row three");
                    }
                    Ok(row)
                },
                0.0,
            )?))
        },
        |_| leaf(),
    )
    .unwrap();
    assert_eq!(
        crate::context::live_node_count(),
        baseline + 1 + 1 + 3,
        "the boundary, the list and one row"
    );

    items.set(vec![1, 2, 3]);
    assert!(boundary.has_failed());
    assert_eq!(crate::context::live_node_count(), baseline + 2);
}

#[test]
fn a_boundary_whose_build_fails_keeps_nothing_of_it() {
    use crate::test_support::{Failure, live, stateful_leaf};

    reset_layout_runtime();
    let bare =
        || ErrorBoundary::new(|| Err(LayoutError::Engine("bare".into())), |_| leaf()).unwrap();
    // The first build on a thread also mints the surface root owner, which is not the boundary's.
    let _warm = bare();
    let before = live();
    let _bare = bare();
    let bare = live() - before;

    for fail in [Failure::Panic, Failure::Err, Failure::Panic, Failure::Err] {
        let before = live();
        let boundary = ErrorBoundary::new(move || stateful_leaf(fail), |_| leaf()).unwrap();
        assert!(boundary.has_failed());
        assert_eq!(
            live() - before,
            bare,
            "only the boundary and its fallback remain"
        );
    }
}
