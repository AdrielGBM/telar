//! A view follows what it reads, which holds only if every ambient value a view can read is a signal: one held anywhere else would be read once and never again, with nothing to say so.

use crate::{
    Direction, Reactive, reads_state, reset_runtime, set_direction, set_locale,
    set_reduced_motion_override, set_surface_size,
};

fn read<T>(hook: impl FnOnce() -> T) -> bool {
    reads_state(hook).1
}

#[test]
fn every_ambient_value_a_view_reads_is_state() {
    reset_runtime();
    let unread: Vec<&str> = [
        ("use_theme_tokens", read(crate::use_theme_tokens)),
        ("use_mode", read(crate::use_mode)),
        ("use_resolved_scheme", read(crate::use_resolved_scheme)),
        ("use_direction", read(crate::use_direction)),
        ("use_locale", read(crate::use_locale)),
        ("use_surface_size", read(crate::use_surface_size)),
        ("use_surface_width", read(crate::use_surface_width)),
        ("use_surface_height", read(crate::use_surface_height)),
        ("use_safe_area_insets", read(crate::use_safe_area_insets)),
        (
            "use_surface_maximized",
            read(ui_core::use_surface_maximized),
        ),
        (
            "use_system_preferences",
            read(crate::use_system_preferences),
        ),
        ("use_color_scheme", read(crate::use_color_scheme)),
        ("use_reduced_motion", read(crate::use_reduced_motion)),
        ("use_high_contrast", read(crate::use_high_contrast)),
        ("use_preferred_locales", read(crate::use_preferred_locales)),
        ("use_text_scale", read(crate::use_text_scale)),
    ]
    .into_iter()
    .filter_map(|(name, read)| (!read).then_some(name))
    .collect();
    assert!(unread.is_empty(), "read without following: {unread:?}");
}

#[test]
fn a_reading_of_an_ambient_value_follows_it_through_a_plain_function() {
    reset_runtime();
    fn mirrored() -> bool {
        crate::use_direction() == Direction::Rtl
    }
    let direction = Reactive::follow(mirrored);
    let locale = Reactive::follow(|| crate::use_locale().map(|tag| tag.to_string()));
    let width = Reactive::follow(crate::use_surface_width);
    let reduced = Reactive::follow(crate::use_reduced_motion);

    set_direction(Direction::Rtl);
    set_locale(crate::locale_tag!("es"));
    set_surface_size(crate::Size::new(321.0, 100.0));
    set_reduced_motion_override(Some(true));

    assert!(direction.get());
    assert_eq!(locale.get().as_deref(), Some("es"));
    assert_eq!(width.get(), 321.0);
    assert_eq!(reduced.get(), Some(true));
    set_direction(Direction::Ltr);
    set_reduced_motion_override(None);
}
