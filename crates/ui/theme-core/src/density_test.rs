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
    set_control_size(ControlSize::Regular);
}
