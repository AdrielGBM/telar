use super::*;

#[test]
fn the_ambient_size_scales_the_bases_and_regular_leaves_them_alone() {
    assert_eq!(ControlSize::Regular.scale(), 1.0);
    assert!(
        ControlSize::Mini.scale() < 1.0,
        "mini is smaller than regular: {}",
        ControlSize::Mini.scale()
    );
    assert!(
        ControlSize::Large.scale() > 1.0,
        "and large is bigger: {}",
        ControlSize::Large.scale()
    );

    assert_eq!(use_control_size(), ControlSize::Regular, "the default");
    set_control_size(ControlSize::Mini);
    assert_eq!(control_scale(), ControlSize::Mini.scale());
    assert_eq!(current_control_size(), ControlSize::Mini);
    set_control_size(ControlSize::Regular);
}

#[test]
fn setting_the_size_already_in_force_reruns_nobody() {
    set_control_size(ControlSize::Regular);
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let s = seen.clone();
    let _e = reactive_core::effect(move || s.borrow_mut().push(use_control_size()));
    set_control_size(ControlSize::Small);
    set_control_size(ControlSize::Small);
    assert_eq!(
        *seen.borrow(),
        vec![ControlSize::Regular, ControlSize::Small]
    );
    set_control_size(ControlSize::Regular);
}

#[test]
fn a_surface_control_size_shadows_the_thread_one_until_cleared() {
    set_control_size(ControlSize::Regular);
    let surface = ControlSizeContext::new();
    {
        let _entered = surface.enter();
        set_surface_control_size(Some(ControlSize::Mini));
        assert_eq!(use_control_size(), ControlSize::Mini);
        assert_eq!(use_surface_control_size(), Some(ControlSize::Mini));
    }
    assert_eq!(
        use_control_size(),
        ControlSize::Regular,
        "the ambient world keeps the thread's size"
    );

    set_control_size(ControlSize::Large);
    {
        let _entered = surface.enter();
        assert_eq!(use_control_size(), ControlSize::Mini);
        set_surface_control_size(None);
        assert_eq!(
            use_control_size(),
            ControlSize::Large,
            "cleared, the surface follows the thread again"
        );
    }
    set_control_size(ControlSize::Regular);
}

#[test]
fn a_provided_size_shadows_the_surface_and_the_thread_for_its_subtree_only() {
    set_control_size(ControlSize::Regular);
    let _app = reactive_core::owner_scope();
    let toolbar = reactive_core::owner_scope();
    let toolbar_owner = toolbar.id();
    drop(toolbar);
    let form = reactive_core::owner_scope();
    ScopedControlSize::new(ControlSize::Large).provide();
    assert_eq!(use_control_size(), ControlSize::Large);
    assert_eq!(current_control_size(), ControlSize::Large);
    assert_eq!(control_scale(), ControlSize::Large.scale());

    let surface = ControlSizeContext::new();
    {
        let _entered = surface.enter();
        set_surface_control_size(Some(ControlSize::Mini));
        assert_eq!(
            use_control_size(),
            ControlSize::Large,
            "the subtree's own size is nearer than the surface's"
        );
    }
    drop(form);

    reactive_core::with_owner(Some(toolbar_owner), || {
        assert_eq!(
            use_control_size(),
            ControlSize::Regular,
            "a sibling subtree keeps the thread's size"
        );
    });
}

#[test]
fn a_nested_scope_wins_and_a_set_re_runs_only_its_readers() {
    set_control_size(ControlSize::Regular);
    let _outer = reactive_core::owner_scope();
    let outer = ScopedControlSize::new(ControlSize::Small);
    outer.provide();
    let _inner = reactive_core::owner_scope();
    let inner = ScopedControlSize::new(ControlSize::Mini);
    inner.provide();

    let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink = seen.clone();
    let _e = reactive_core::effect(move || sink.borrow_mut().push(use_control_size()));
    outer.set(ControlSize::Large);
    inner.set(ControlSize::Mini);
    inner.set(ControlSize::Large);
    set_control_size(ControlSize::Small);
    assert_eq!(
        *seen.borrow(),
        vec![ControlSize::Mini, ControlSize::Large],
        "only the nearest scope's change reaches the reader, and an unchanged set reaches nobody"
    );
    set_control_size(ControlSize::Regular);
}
