use std::cell::{Cell, RefCell};
use std::time::Duration;

use layout_core::AvailableSpace;
use motion_core::tick;
use platform_core::{PointerButton, PointerSource};
use reactive_core::{effect, signal};
use renderer_core::RectStyle;
use web_time::Instant;

use super::*;
use crate::container::Container;
use crate::context::{compute_layout, live_node_count, reset_layout_runtime};
use crate::error_boundary::ErrorBoundary;
use crate::focus;
use crate::input_region::{interactive_rects, mentions};
use crate::layout_item::box_item;
use crate::styled_container::StyledContainer;

const MS: u64 = 200;

fn fade() -> Transition {
    Transition::fade(tween(Duration::from_millis(MS), Easing::Linear))
}

fn leaf() -> Result<Box<dyn LayoutItem>, LayoutError> {
    Ok(Box::new(Container::new(
        LayoutStyle::new().width(10.0).height(10.0),
        vec![],
    )?))
}

fn button(presses: Rc<Cell<u32>>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    Ok(box_item(
        StyledContainer::new(
            LayoutStyle::new().width(40.0).height(40.0),
            |_| RectStyle::default(),
            vec![],
        )?
        .on_press(move || presses.set(presses.get() + 1))
        .control(focus::Role::Button),
    ))
}

fn press(target: &mut dyn LayoutItem) -> EventResult {
    let at = |event: fn(f64, f64, PointerButton, PointerSource) -> Event| {
        event(20.0, 20.0, PointerButton::Primary, PointerSource::Mouse)
    };
    let result = target.on_event(&at(|x, y, button, source| Event::PointerPressed {
        x,
        y,
        button,
        source,
    }));
    target.on_event(&at(|x, y, button, source| Event::PointerReleased {
        x,
        y,
        button,
        source,
    }));
    result
}

/// Plays whatever is animating for `ms` from `start`, as the runner's frames would.
fn play(start: Instant, ms: u64) -> Instant {
    tick(start);
    let end = start + Duration::from_millis(ms);
    tick(end);
    end
}

fn child_node(presence: &Presence) -> NodeId {
    presence
        .state
        .borrow()
        .child
        .as_ref()
        .map(Child::node)
        .expect("a child is mounted")
}

#[test]
fn a_hidden_child_takes_no_input_while_it_leaves_and_is_gone_after_its_exit() {
    reset_layout_runtime();
    focus::clear();
    let visible = signal(false);
    let presses = Rc::new(Cell::new(0));
    let mut presence = {
        let presses = Rc::clone(&presses);
        Presence::new(
            move || visible.get(),
            fade(),
            move || button(Rc::clone(&presses)),
        )
        .unwrap()
    };
    let baseline = live_node_count();
    assert!(!presence.is_mounted(), "nothing is built while hidden");

    visible.set(true);
    let shown = play(Instant::now(), MS + 50);
    compute_layout(
        presence.layout_node(),
        AvailableSpace::Definite(100.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();
    assert_eq!(press(&mut presence), EventResult::Handled);
    assert_eq!(presses.get(), 1);
    assert!(!interactive_rects().is_empty());
    focus::focus_next();
    assert!(focus::current().is_some(), "the shown button takes focus");

    visible.set(false);
    assert!(presence.is_mounted(), "still mounted for its exit");
    assert!(focus::current().is_none(), "focus leaves with it");
    assert!(
        interactive_rects().is_empty(),
        "no share of the input region"
    );
    assert!(!presence.occludes(), "what is under it answers");
    assert_eq!(press(&mut presence), EventResult::Ignored);
    assert_eq!(
        presses.get(),
        1,
        "a press on a leaving child reaches nothing"
    );

    let midway = play(shown, MS / 2);
    assert!(presence.is_mounted(), "halfway through the exit");
    play(midway, MS);
    assert!(!presence.is_mounted(), "removed once the exit has played");
    assert_eq!(live_node_count(), baseline, "every node it built is freed");
}

#[test]
fn shown_again_mid_exit_it_turns_back_as_the_same_node_with_the_same_state() {
    reset_layout_runtime();
    let visible = signal(true);
    let builds = Rc::new(Cell::new(0));
    let local: Rc<Cell<Option<RwSignal<i32>>>> = Rc::default();
    let presence = {
        let (builds, local) = (Rc::clone(&builds), Rc::clone(&local));
        Presence::new(
            move || visible.get(),
            fade(),
            move || {
                builds.set(builds.get() + 1);
                local.set(Some(signal(0)));
                leaf()
            },
        )
        .unwrap()
    };
    let state = local.get().expect("built while visible");
    state.set(7);
    let node = child_node(&presence);

    visible.set(false);
    let midway = play(Instant::now(), MS / 2);
    visible.set(true);
    play(midway, MS * 2);

    assert_eq!(builds.get(), 1, "never rebuilt");
    assert_eq!(child_node(&presence), node, "the same node");
    assert_eq!(state.get(), 7, "its own state kept");
    assert!(!presence.state.borrow().child.as_ref().unwrap().is_leaving());
    assert_eq!(exits_in_flight().get(), 0);
}

#[test]
fn exits_in_flight_counts_an_exit_from_its_start_to_its_end() {
    reset_layout_runtime();
    let visible = signal(true);
    let _presence = Presence::new(move || visible.get(), fade(), leaf).unwrap();
    let seen: Rc<RefCell<Vec<usize>>> = Rc::default();
    let _watch = {
        let seen = Rc::clone(&seen);
        effect(move || seen.borrow_mut().push(exits_in_flight().get()))
    };

    visible.set(false);
    play(Instant::now(), MS + 50);

    assert_eq!(*seen.borrow(), vec![0, 1, 0]);
}

#[test]
fn a_transition_with_no_duration_removes_the_child_at_once() {
    reset_layout_runtime();
    let visible = signal(true);
    let instant = Transition::fade(tween(Duration::ZERO, Easing::Linear));
    let presence = Presence::new(move || visible.get(), instant, leaf).unwrap();

    visible.set(false);

    assert!(!presence.is_mounted());
    assert_eq!(exits_in_flight().get(), 0);
}

#[test]
fn a_leaving_child_inside_a_failing_boundary_is_cleaned_up() {
    reset_layout_runtime();
    let visible = signal(true);
    let boom = signal(false);
    let leaving: Rc<Cell<Option<NodeId>>> = Rc::default();
    let baseline = live_node_count();

    let boundary = {
        let leaving = Rc::clone(&leaving);
        ErrorBoundary::new(
            move || {
                effect(move || {
                    if boom.get() {
                        panic!("boom");
                    }
                });
                Ok(box_item(Presence::new(
                    move || visible.get(),
                    fade(),
                    move || {
                        let built = leaf()?;
                        leaving.set(Some(built.layout_node()));
                        Ok(built)
                    },
                )?))
            },
            |_| leaf(),
        )
        .unwrap()
    };
    assert_eq!(
        live_node_count(),
        baseline + 3,
        "boundary, presence and child"
    );
    let leaving = leaving.get().expect("the child was built");

    visible.set(false);
    assert_eq!(exits_in_flight().get(), 1);
    assert!(mentions(leaving), "the leaving child is gated");

    boom.set(true);
    assert!(boundary.has_failed());
    assert_eq!(
        exits_in_flight().get(),
        0,
        "the exit is no longer waited on"
    );
    assert_eq!(
        live_node_count(),
        baseline + 2,
        "boundary and fallback; the leaving child is freed"
    );
    assert!(!mentions(leaving), "its input registrations are withdrawn");
    play(Instant::now(), MS + 50);
    assert_eq!(live_node_count(), baseline + 2);
}

#[test]
fn a_child_that_fails_to_build_leaves_nothing_behind() {
    use crate::test_support::{Failure, live, stateful_leaf};

    reset_layout_runtime();
    let show = signal(false);
    let fail = Rc::new(Cell::new(Failure::Panic));
    let presence = {
        let fail = Rc::clone(&fail);
        Presence::new(
            move || show.get(),
            fade(),
            move || stateful_leaf(fail.get()),
        )
        .unwrap()
    };
    let baseline = live();

    for attempt in 0..6 {
        fail.set(if attempt % 2 == 0 {
            Failure::Panic
        } else {
            Failure::Err
        });
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| show.set(true)));
        assert!(outcome.is_err(), "no boundary caught attempt {attempt}");
        show.set(false);
        assert_eq!(live(), baseline, "attempt {attempt} left something behind");
    }

    fail.set(Failure::None);
    show.set(true);
    assert!(presence.is_mounted());
}

#[test]
fn an_exit_turned_back_returns_in_the_time_it_had_spent_leaving() {
    reset_layout_runtime();
    motion_core::reset();
    let visible = signal(true);
    let presence = Presence::new(move || visible.get(), fade(), leaf).unwrap();
    let mount = || {
        presence
            .state
            .borrow()
            .child
            .as_ref()
            .and_then(Child::mount)
            .expect("presented")
    };

    visible.set(false);
    let start = Instant::now();
    tick(start);
    let turned = start + Duration::from_millis(MS / 4);
    tick(turned);
    assert!((mount().frame_now().1 - 0.75).abs() < 1e-3);

    visible.set(true);
    tick(turned + Duration::from_millis(MS / 8));
    assert!(
        (mount().frame_now().1 - 0.875).abs() < 1e-3,
        "halfway back after half the time it spent leaving: {}",
        mount().frame_now().1
    );
    tick(turned + Duration::from_millis(MS / 4 + 1));
    assert_eq!(
        mount().frame_now().1,
        1.0,
        "back in a quarter of the duration"
    );
}

/// A build that writes what `visible` reads re-runs the effect before the first build has mounted; that run waits for the first to commit instead of building a second child beside it.
#[test]
fn a_build_that_writes_what_visible_reads_builds_one_child() {
    reset_layout_runtime();
    let pokes = signal(0u32);
    let builds = Rc::new(Cell::new(0u32));
    let baseline = live_node_count();

    let counted = builds.clone();
    let presence = Presence::new(
        move || {
            pokes.get();
            true
        },
        fade(),
        move || {
            counted.set(counted.get() + 1);
            pokes.set(pokes.peek() + 1);
            leaf()
        },
    )
    .unwrap();

    assert_eq!(builds.get(), 1);
    assert_eq!(
        live_node_count(),
        baseline + 2,
        "the presence and its one child"
    );
    assert_eq!(
        layout_reactive::parent(child_node(&presence)),
        Some(presence.node),
        "and that child is the one mounted"
    );
}
