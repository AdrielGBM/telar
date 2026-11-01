use geometry_core::Size;

use super::*;

#[test]
fn each_length_resolves_against_what_it_is_relative_to() {
    let surface = Size::new(1000.0, 600.0);
    assert_eq!(TextLength::Px(12.0).resolve(16.0, surface), 12.0);
    assert_eq!(TextLength::Em(1.5).resolve(16.0, surface), 24.0);
    assert_eq!(TextLength::SurfaceWidth(0.22).resolve(16.0, surface), 220.0);
    assert_eq!(TextLength::SurfaceHeight(0.1).resolve(16.0, surface), 60.0);
    assert_eq!(TextLength::SurfaceMin(0.1).resolve(16.0, surface), 60.0);
    assert_eq!(TextLength::SurfaceMax(0.1).resolve(16.0, surface), 100.0);
}

#[test]
fn a_fraction_of_the_surface_becomes_pixels_on_one_and_em_waits_for_its_base() {
    let surface = Size::new(1000.0, 600.0);
    assert_eq!(
        TextLength::SurfaceWidth(0.5).on_surface(surface),
        TextLength::Px(500.0)
    );
    assert_eq!(TextLength::Em(2.0).on_surface(surface), TextLength::Em(2.0));
}
