use reactive_core::{current_surface, effect, signal};

use super::Surface;

// Regression: with the parent links in one ambient map, namesakes from different surfaces overwrote each other, so a climb begun in one surface walked another's tree and hung looking for a root not on its path.
#[test]
fn one_surfaces_parent_links_never_answer_for_another() {
    use layout_reactive::{LayoutStyle, new_container, parent};

    let a = Surface::new();
    let b = Surface::new();

    let (child_in_a, host_in_a) = {
        let _guard = a.enter();
        let child = new_container(LayoutStyle::new(), &[]).unwrap();
        let host = new_container(LayoutStyle::new(), &[child]).unwrap();
        assert_eq!(parent(child), Some(host));
        (child, host)
    };

    let _guard = b.enter();
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
    let a_node = {
        let _g = a.enter();
        let (node, _) = new_leaf(LayoutStyle::new().width(10.0).height(10.0)).unwrap();
        effect(move || {
            read.get();
            ran_c.borrow_mut().push(current_surface().0);
        });
        node
    };

    ran_under.borrow_mut().clear();

    {
        let _g = b.enter();
        shared.set(1);
    }
    assert_eq!(
        ran_under.borrow().as_slice(),
        &[a.handle().0],
        "A's effect must run under A's surface, not B's"
    );

    {
        let _g = a.enter();
        compute_layout(
            a_node,
            AvailableSpace::Definite(100.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();
        assert_eq!(track_layout(a_node).unwrap().get().width, 10.0);
    }
    {
        let _g = b.enter();
        assert!(
            track_layout(a_node).is_none(),
            "A's node must not exist in B's layout world"
        );
    }
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
    {
        let _g = other.enter();
        shared.set(1);
    }
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

    {
        let _g = a.enter();
        provide(String::from("A")).unwrap();
        assert_eq!(try_inject::<String>().as_deref(), Some("A"));
    }
    {
        let _g = b.enter();
        provide(String::from("B")).unwrap();
        assert_eq!(try_inject::<String>().as_deref(), Some("B"));
    }

    let shared = signal(0i32);
    let read = shared.read_only();
    let seen: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let seen_c = Rc::clone(&seen);
    {
        let _g = a.enter();
        effect(move || {
            read.get();
            seen_c
                .borrow_mut()
                .push(try_inject::<String>().unwrap_or_default());
        })
    };

    seen.borrow_mut().clear();
    {
        let _g = b.enter();
        shared.set(1);
    }
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
    {
        let _g = a.enter();
        effect(move || {
            read_a.get();
            log_a.borrow_mut().push(('a', current_surface().0));
        })
    };

    let log_b = Rc::clone(&log);
    let read_b = global.read_only();
    {
        let _g = b.enter();
        effect(move || {
            read_b.get();
            log_b.borrow_mut().push(('b', current_surface().0));
        })
    };

    log.borrow_mut().clear();

    global.set(1);

    let entries = log.borrow().clone();
    assert!(
        entries.contains(&('a', a.handle().0)),
        "A's effect must re-run under A: {entries:?}"
    );
    assert!(
        entries.contains(&('b', b.handle().0)),
        "B's effect must re-run under B: {entries:?}"
    );
}
