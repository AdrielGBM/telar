use geometry_core::Size;

use super::*;

#[test]
fn a_fraction_is_of_the_containing_width_and_waits_for_one() {
    let fit = FontFit::containing(0.6);
    let surface = Size::new(1000.0, 800.0);
    assert_eq!(fit.target(16.0, surface, Some(500.0)), Some((300.0, None)));
    assert_eq!(fit.target(16.0, surface, None), None);
}

#[test]
fn a_length_resolves_like_any_text_length() {
    let surface = Size::new(1000.0, 800.0);
    let fit = FontFit::new(TextLength::SurfaceWidth(0.5)).with_max_height(TextLength::Em(4.0));
    assert_eq!(fit.target(20.0, surface, None), Some((500.0, Some(80.0))));
    assert_eq!(
        FontFit::new(320.0).target(20.0, surface, None),
        Some((320.0, None))
    );
}

#[test]
fn only_a_fraction_of_the_surface_reads_the_surface() {
    assert!(!FontFit::containing(0.6).uses_surface());
    assert!(!FontFit::new(TextLength::Em(10.0)).uses_surface());
    assert!(FontFit::new(TextLength::SurfaceWidth(0.4)).uses_surface());
    assert!(
        FontFit::containing(0.6)
            .with_max_height(TextLength::SurfaceHeight(0.56))
            .uses_surface()
    );
}
