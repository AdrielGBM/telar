use super::*;

#[test]
fn the_package_tolerance_overrides_the_default_key_by_key() {
    let options = TestOptions {
        channel_tolerance: Some(6),
        ..TestOptions::default()
    };
    assert_eq!(tolerance(&options), Tolerance::new(6, 0));
    assert_eq!(tolerance(&TestOptions::default()), Tolerance::default());
}
