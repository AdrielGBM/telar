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
