use reactive_core::{current_surface, effect, signal};

use super::Surface;

// Regression: with the parent links in one ambient map, namesakes from different surfaces overwrote each other, so a climb begun in one surface walked another's tree and hung looking for a root not on its path.
#[test]
fn one_surfaces_parent_links_never_answer_for_another() {
    use layout_reactive::{LayoutStyle, new_container, parent};

    let a = Surface::new();
    let b = Surface::new();

    let (child_in_a, host_in_a) = a.with_entered(|| {
        let child = new_container(LayoutStyle::new(), &[]).unwrap();
        let host = new_container(LayoutStyle::new(), &[child]).unwrap();
        assert_eq!(parent(child), Some(host));
        (child, host)
    });

    b.with_entered(|| {
        let child_in_b = new_container(LayoutStyle::new(), &[]).unwrap();
        assert_eq!(
            child_in_b, child_in_a,
            "the two surfaces must really collide on ids, or this proves nothing"
        );
        assert_eq!(
            parent(child_in_b),
            None,
            "a's link answered for b's node — the namesake, not the node"
        );
        assert_ne!(parent(child_in_b), Some(host_in_a));
    })
}

#[test]
fn effect_reenters_its_surface_layout_world() {
    use layout_reactive::{AvailableSpace, LayoutStyle, compute_layout, new_leaf, track_layout};
    use std::cell::RefCell;
    use std::rc::Rc;

    let a = Surface::new();
    let b = Surface::new();
    assert_ne!(a.handle(), b.handle());
    assert!(
        !a.handle().is_none(),
        "the effect reached its own surface's layout world"
    );

    let shared = signal(0i32);

    let ran_under: Rc<RefCell<Vec<u64>>> = Rc::new(RefCell::new(Vec::new()));
    let ran_c = Rc::clone(&ran_under);
    let read = shared.read_only();
    let a_node = a.with_entered(|| {
        let (node, _) = new_leaf(LayoutStyle::new().width(10.0).height(10.0)).unwrap();
        effect(move || {
            read.get();
            ran_c.borrow_mut().push(current_surface().raw());
        });
        node
    });

    ran_under.borrow_mut().clear();

    b.with_entered(|| {
        shared.set(1);
    });
    assert_eq!(
        ran_under.borrow().as_slice(),
        &[a.handle().raw()],
        "A's effect must run under A's surface, not B's"
    );

    a.with_entered(|| {
        compute_layout(
            a_node,
            AvailableSpace::Definite(100.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();
        assert_eq!(track_layout(a_node).unwrap().get().width, 10.0);
    });
    b.with_entered(|| {
        assert!(
            track_layout(a_node).is_none(),
            "A's node must not exist in B's layout world"
        );
    });
}

// An effect belonging to no surface has the ambient world, and every effect of a single-window app is one: the runner builds no `Surface`. Left un-restored they ran against whichever surface was active.
#[test]
fn an_effect_owned_by_no_surface_reenters_the_ambient_world() {
    use layout_reactive::{AvailableSpace, LayoutStyle, compute_layout, new_leaf, track_layout};
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::Surface;

    let (ambient_node, _) = new_leaf(LayoutStyle::new().width(42.0).height(10.0)).unwrap();
    compute_layout(
        ambient_node,
        AvailableSpace::Definite(100.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();

    let other = Surface::new();
    let shared = signal(0i32);
    let read = shared.read_only();
    let seen: Rc<RefCell<Vec<Option<f32>>>> = Rc::new(RefCell::new(Vec::new()));
    let seen_c = Rc::clone(&seen);
    effect(move || {
        read.get();
        seen_c
            .borrow_mut()
            .push(track_layout(ambient_node).map(|rect| rect.get().width));
    });

    seen.borrow_mut().clear();
    other.with_entered(|| {
        shared.set(1);
    });
    assert_eq!(
        seen.borrow().as_slice(),
        &[Some(42.0)],
        "an ambient effect must resolve against the ambient layout world, not the active surface's"
    );
}

#[test]
fn provide_inject_is_per_surface_and_survives_into_effects() {
    use std::cell::RefCell;
    use std::rc::Rc;

    use services_core::{provide, try_inject};

    let a = Surface::new();
    let b = Surface::new();

    a.with_entered(|| {
        provide(String::from("A")).unwrap();
        assert_eq!(try_inject::<String>().as_deref(), Some("A"));
    });
    b.with_entered(|| {
        provide(String::from("B")).unwrap();
        assert_eq!(try_inject::<String>().as_deref(), Some("B"));
    });

    let shared = signal(0i32);
    let read = shared.read_only();
    let seen: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let seen_c = Rc::clone(&seen);
    a.with_entered(|| {
        effect(move || {
            read.get();
            seen_c
                .borrow_mut()
                .push(try_inject::<String>().unwrap_or_default());
        })
    });

    seen.borrow_mut().clear();
    b.with_entered(|| {
        shared.set(1);
    });
    assert_eq!(
        seen.borrow().as_slice(),
        &[String::from("A")],
        "A's effect must inject A's context even when fired from B"
    );
}

#[test]
fn global_signal_reruns_all_surfaces_each_under_its_context() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let a = Surface::new();
    let b = Surface::new();

    let global = signal(0i32);

    let log: Rc<RefCell<Vec<(char, u64)>>> = Rc::new(RefCell::new(Vec::new()));

    let log_a = Rc::clone(&log);
    let read_a = global.read_only();
    a.with_entered(|| {
        effect(move || {
            read_a.get();
            log_a.borrow_mut().push(('a', current_surface().raw()));
        })
    });

    let log_b = Rc::clone(&log);
    let read_b = global.read_only();
    b.with_entered(|| {
        effect(move || {
            read_b.get();
            log_b.borrow_mut().push(('b', current_surface().raw()));
        })
    });

    log.borrow_mut().clear();

    global.set(1);

    let entries = log.borrow().clone();
    assert!(
        entries.contains(&('a', a.handle().raw())),
        "A's effect must re-run under A: {entries:?}"
    );
    assert!(
        entries.contains(&('b', b.handle().raw())),
        "B's effect must re-run under B: {entries:?}"
    );
}

#[test]
fn each_surface_reads_its_own_cascade() {
    use crate::inherit::{declare, inherited_text_style};
    use layout_reactive::{LayoutStyle, new_container};
    use renderer_core::Declared;

    let a = Surface::new();
    let b = Surface::new();

    let in_a = a.with_entered(|| {
        let node = new_container(LayoutStyle::new(), &[]).unwrap();
        declare(node, Declared::default().with_font_size(11.0));
        node
    });
    let in_b = b.with_entered(|| {
        let node = new_container(LayoutStyle::new(), &[]).unwrap();
        declare(node, Declared::default().with_font_size(22.0));
        node
    });
    assert_eq!(
        in_a, in_b,
        "the two surfaces must share the node id, or this proves nothing"
    );

    a.with_entered(|| {
        assert_eq!(inherited_text_style(in_a).font_size, 11.0);
    });
    b.with_entered(|| {
        assert_eq!(inherited_text_style(in_b).font_size, 22.0);
    })
}

#[test]
fn dropping_a_surface_frees_its_worlds() {
    use layout_reactive::{
        AvailableSpace, LayoutStyle, absolute_rect, compute_layout, new_leaf, set_overlay_host,
    };
    use reactive_core::{live_effect_count, live_owner_count, live_signal_count};

    drop(Surface::new());
    // `compute_layout` reads the thread-global text direction, which mints its own signal (not per-surface, and outside the scope of this test) on first read. Warm it up before the baseline so the loop below only measures what the surface itself owns.
    {
        let warm = Surface::new();
        warm.with_entered(|| {
            let (node, _) = new_leaf(LayoutStyle::new()).unwrap();
            compute_layout(
                node,
                AvailableSpace::Definite(1.0),
                AvailableSpace::Definite(1.0),
            )
            .unwrap();
        })
    }
    let signals = live_signal_count();
    let effects = live_effect_count();
    let owners = live_owner_count();

    for _ in 0..8 {
        let surface = Surface::new();
        surface.with_entered(|| {
            crate::inherit::reset_cascade();
            let exits = crate::presence::exits_in_flight();
            effect(move || {
                exits.get();
            });

            let id = crate::focus::next_id();
            crate::focus::request(id);
            assert!(
                crate::focus::is_focused(id),
                "focus must land inside the surface it was requested in"
            );

            let (node, _) = new_leaf(LayoutStyle::new().width(10.0).height(10.0)).unwrap();
            compute_layout(
                node,
                AvailableSpace::Definite(100.0),
                AvailableSpace::Definite(100.0),
            )
            .unwrap();
            set_overlay_host(node);
            assert!(
                absolute_rect(node).is_some(),
                "absolute_rect must mint the node's position signal"
            );
        });
        drop(surface);
    }

    assert_eq!(live_signal_count(), signals);
    assert_eq!(live_effect_count(), effects);
    assert_eq!(live_owner_count(), owners);
}

#[test]
fn after_settle_skips_a_surface_dropped_before_it_settles() {
    use std::cell::Cell;
    use std::rc::Rc;

    let ran = Rc::new(Cell::new(false));
    let ran_c = Rc::clone(&ran);

    reactive_core::batch(|| {
        let surface = Surface::new();
        surface.with_entered(|| {
            reactive_core::after_settle(move || {
                ran_c.set(true);
            });
        })
    });

    assert!(
        !ran.get(),
        "after_settle must not run a closure whose surface was disposed before the batch settled"
    );
}

#[test]
fn a_component_reads_the_size_of_the_surface_it_was_built_on() {
    use geometry_core::Size;
    use layout_reactive::{set_surface_size, use_surface_width};
    use std::cell::RefCell;
    use std::rc::Rc;

    let window = Surface::new();
    let panel = Surface::new();
    window.with_entered(|| {
        set_surface_size(Size::new(1280.0, 800.0));
    });
    panel.with_entered(|| {
        set_surface_size(Size::new(320.0, 800.0));
    });

    let seen: Rc<RefCell<Vec<f32>>> = Rc::default();
    let log = Rc::clone(&seen);
    window.with_entered(|| {
        effect(move || log.borrow_mut().push(use_surface_width()));
    });
    panel.with_entered(|| {
        set_surface_size(Size::new(400.0, 800.0));
    });
    window.with_entered(|| {
        set_surface_size(Size::new(1024.0, 800.0));
    });
    panel.with_entered(|| {
        set_surface_size(Size::new(360.0, 800.0));
        assert_eq!(
            *seen.borrow(),
            [1280.0, 1024.0],
            "the panel resizing is not the window resizing"
        );
    })
}

/// A control, a modal over a 100×100 root and the root, built on the active surface: the first focus id and the first overlay id that surface mints, so another surface's twins are their namesakes.
struct Widgets {
    _control: crate::StyledContainer,
    focus: crate::focus::FocusId,
    node: layout_reactive::NodeId,
    _modal: crate::Overlay,
}

fn widgets() -> Widgets {
    use crate::LayoutItem;
    use layout_reactive::{AvailableSpace, LayoutStyle, compute_layout, new_container};

    let control = crate::StyledContainer::new(
        LayoutStyle::new().width(40.0).height(20.0),
        |_| renderer_core::RectStyle::default(),
        vec![],
    )
    .unwrap()
    .control(crate::focus::Role::Button)
    .on_press(|| {});
    let panel = crate::StyledContainer::new(
        LayoutStyle::new().width(50.0).height(20.0),
        |_| renderer_core::RectStyle::default(),
        vec![],
    )
    .unwrap();
    let modal = crate::Overlay::new(LayoutStyle::new(), vec![Box::new(panel)]).unwrap();
    let root = new_container(
        LayoutStyle::new().width(100.0).height(100.0),
        &[control.layout_node(), modal.layout_node()],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(100.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();
    Widgets {
        focus: control.focus_id().expect("a control has a focus id"),
        node: control.layout_node(),
        _control: control,
        _modal: modal,
    }
}

fn modal_takes_a_press() -> bool {
    use platform_core::{Event, PointerButton, PointerSource};
    let press = Event::PointerPressed {
        x: 10.0,
        y: 10.0,
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    };
    ui_tree::dispatch_overlays(&press) == ui_tree::EventResult::Handled
}

fn untouched(widgets: &Widgets) {
    assert!(
        crate::focus::is_registered(widgets.focus),
        "the namesake focus stop is still there"
    );
    assert!(
        crate::input_region::mentions(widgets.node),
        "and so is its input region"
    );
    assert!(modal_takes_a_press(), "and the namesake modal still blocks");
}

#[test]
fn a_widget_dropped_after_its_surface_is_gone_leaves_the_active_surface_alone() {
    let home = Surface::new();
    let away = Surface::new();
    let gone = home.with_entered(widgets);
    drop(home);

    away.with_entered(|| {
        let here = widgets();
        assert_eq!(
            (here.focus, here.node),
            (gone.focus, gone.node),
            "the two surfaces must really collide on ids, or this proves nothing"
        );
        drop(gone);

        untouched(&here);
    })
}

#[test]
fn a_widget_dropped_on_another_surfaces_turn_withdraws_from_its_own() {
    let home = Surface::new();
    let away = Surface::new();
    let (leaving, leaving_node) = home.with_entered(|| {
        let widgets = widgets();
        let node = widgets.node;
        (widgets, node)
    });

    away.with_entered(|| {
        let here = widgets();
        drop(leaving);
        untouched(&here);
    });

    home.with_entered(|| {
        assert!(
            crate::focus::tab_order().is_empty(),
            "its focus stop left its own surface"
        );
        assert!(
            !crate::input_region::mentions(leaving_node),
            "and so did its input region"
        );
        assert!(!modal_takes_a_press(), "and its modal");
    })
}

struct FreesNode(layout_reactive::NodeId);

impl Drop for FreesNode {
    fn drop(&mut self) {
        layout_reactive::remove_node(self.0);
    }
}

#[test]
fn an_owner_disposed_on_another_surfaces_turn_tears_down_inside_its_own() {
    use layout_reactive::{LayoutStyle, live_node_count, new_container};
    use reactive_core::{dispose_owner, on_cleanup, owner_scope, signal};

    let home = Surface::new();
    let away = Surface::new();
    let owner = home.with_entered(|| {
        let scope = owner_scope();
        let held = new_container(LayoutStyle::new(), &[]).unwrap();
        let by_cleanup = new_container(LayoutStyle::new(), &[]).unwrap();
        let _value = signal(FreesNode(held));
        on_cleanup(move || drop(FreesNode(by_cleanup)));
        assert_eq!(live_node_count(), 2);
        scope.id()
    });

    away.with_entered(|| {
        let _namesake = new_container(LayoutStyle::new(), &[]).unwrap();
        let _other = new_container(LayoutStyle::new(), &[]).unwrap();
        assert_eq!(live_node_count(), 2);

        dispose_owner(owner);

        assert_eq!(
            live_node_count(),
            2,
            "nothing of the owner's was withdrawn from the surface active now"
        );
    });

    home.with_entered(|| {
        assert_eq!(
            live_node_count(),
            0,
            "the owner's nodes left their own surface"
        );
    })
}

/// A window from `open_window` or the multi-window runner is a bare `Surface`, with no canvas around it to cancel what it started: closing it has to, or its work finishes later and writes its freed state under whichever window is active then.
#[test]
fn dropping_a_surface_cancels_its_tasks_and_timers_and_leaves_anothers() {
    use reactive_core::{
        advance_timer_clock, drain_tasks, fire_timers, run_after, signal, spawn_task,
    };
    use std::time::Duration;

    let main = Surface::new();
    let secondary = Surface::new();
    let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();

    let (task, timer, written) = secondary.with_entered(|| {
        let written = signal(0u32);
        let task = spawn_task(
            move || {
                release_rx.recv().ok();
            },
            move |()| written.set(1),
        );
        let timer = run_after(Duration::from_millis(10), move || written.set(2));
        (task, timer, written)
    });
    let (main_task, main_timer, main_written) = main.with_entered(|| {
        let main_written = signal(0u32);
        let main_task = spawn_task(|| (), move |()| main_written.update(|n| *n += 1));
        let main_timer = run_after(Duration::from_millis(10), move || {
            main_written.update(|n| *n += 1)
        });
        (main_task, main_timer, main_written)
    });

    drop(secondary);
    assert!(!task.is_pending(), "its task is cancelled when it closes");
    assert!(
        !timer.is_pending(),
        "and so is its timer, though its handle is still held"
    );
    assert!(
        !written.is_alive(),
        "what their callbacks would write is gone with it"
    );

    release_tx.send(()).ok();
    main.with_entered(|| {
        advance_timer_clock(Duration::from_millis(10));
        fire_timers();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while main_task.is_pending() {
            assert!(
                std::time::Instant::now() < deadline,
                "the main window's task never completed"
            );
            std::thread::sleep(Duration::from_millis(1));
            drain_tasks();
        }
        std::thread::sleep(Duration::from_millis(20));
        drain_tasks();

        assert!(!main_timer.is_pending());
        assert_eq!(
            main_written.get(),
            2,
            "the window still open keeps its task and its timer"
        );
    })
}

#[test]
fn nested_surfaces_each_resolve_their_own_world_and_restore_the_one_around_them() {
    use layout_reactive::{LayoutStyle, new_leaf, track_layout};

    let a = Surface::new();
    let b = Surface::new();
    let (ambient, _) = new_leaf(LayoutStyle::new().width(1.0)).unwrap();
    a.with_entered(|| {
        let (in_a, _) = new_leaf(LayoutStyle::new().width(2.0)).unwrap();
        b.with_entered(|| {
            assert_eq!(current_surface(), b.handle());
            assert!(
                track_layout(in_a).is_none(),
                "b's tree does not know a's node"
            );
        });
        assert_eq!(current_surface(), a.handle());
        assert!(track_layout(in_a).is_some(), "a's tree is live again");
    });
    assert!(current_surface().is_none());
    assert!(track_layout(ambient).is_some());
}

/// A panic inside a nested surface unwinds through its entry, and the outer surface's world is live again once the panic is caught: a build that failed inside a frame must not leave the next widget built in the frame's world.
#[test]
fn an_unwinding_entry_restores_the_surface_around_it() {
    use layout_reactive::{LayoutStyle, new_leaf, track_layout};

    let outer = Surface::new();
    let inner = Surface::new();
    outer.with_entered(|| {
        let (in_outer, _) = new_leaf(LayoutStyle::new()).unwrap();
        let quiet = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            inner.with_entered(|| panic!("unwinds through the inner surface"))
        }));
        std::panic::set_hook(quiet);
        assert!(outcome.is_err());
        assert_eq!(current_surface(), outer.handle());
        assert!(track_layout(in_outer).is_some());
    });
    assert!(current_surface().is_none());
}

/// The reactive flush re-enters an effect's surface through the hook, and an effect may drop the last handle to that surface — closing the window it runs in. The hook holds the surface until the effect returns, so the rest of the effect still reads a live world, and the surface goes after.
#[test]
fn an_effect_that_drops_the_last_handle_to_its_surface_finishes_in_its_world() {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    use layout_reactive::{LayoutStyle, new_leaf, track_layout};

    let holder: Rc<RefCell<Option<Rc<Surface>>>> = Rc::new(RefCell::new(None));
    let surface = Surface::new();
    let handle = surface.handle();
    let close = signal(false);
    let finished_inside = Rc::new(Cell::new(None));
    let _effect = surface.with_entered(|| {
        let (node, _) = new_leaf(LayoutStyle::new()).unwrap();
        let holder = Rc::clone(&holder);
        let finished_inside = Rc::clone(&finished_inside);
        effect(move || {
            if !close.get() {
                return;
            }
            drop(holder.borrow_mut().take());
            finished_inside.set(Some((current_surface(), track_layout(node).is_some())));
        })
    });
    *holder.borrow_mut() = Some(surface);

    close.set(true);

    assert_eq!(finished_inside.get(), Some((handle, true)));
    assert!(
        Surface::find(handle).is_none(),
        "the surface went once the effect returned"
    );
    assert!(current_surface().is_none());
}
