use super::*;
use std::cell::Cell;

use crate::{Container, LayoutStyle};

#[test]
fn new_globals_set_nothing() {
    let globals = Globals::new();
    assert_eq!(globals.mode().get(), None);
    assert_eq!(globals.locale().get(), None);
    assert_eq!(globals.direction().get(), None);
    assert_eq!(globals.viewport().get(), None);
    assert_eq!(globals.background().get(), None);
    assert_eq!(globals.control_size().get(), None);
    assert_eq!(globals.high_contrast().get(), None);
    assert_eq!(globals.safe_area().get(), Insets::default());
}

#[test]
fn a_preview_env_seeds_what_it_names() {
    let env = PreviewEnv::NONE
        .mode("dark")
        .locale("ar")
        .direction(Direction::Rtl)
        .viewport(390.0, 844.0)
        .background(Color::rgba(0.0, 0.0, 0.0, 1.0))
        .high_contrast(true);
    let globals = Globals::seeded(&env);
    assert_eq!(globals.high_contrast().get(), Some(true));
    assert_eq!(globals.mode().get().as_deref(), Some("dark"));
    assert_eq!(globals.locale().get().as_deref(), Some("ar"));
    assert_eq!(globals.direction().get(), Some(Direction::Rtl));
    assert_eq!(globals.viewport().get(), Some(Size::new(390.0, 844.0)));
    assert_eq!(
        globals.background().get(),
        Some(Color::rgba(0.0, 0.0, 0.0, 1.0))
    );
}

#[test]
fn seeding_leaves_what_the_env_does_not_name_as_the_host_set_it() {
    let globals = Globals::new();
    globals.locale().set(Some("es".to_string()));
    globals.control_size().set(Some(ControlSize::Small));
    globals.seed(&PreviewEnv::NONE.mode("dark"));
    assert_eq!(globals.mode().get().as_deref(), Some("dark"));
    assert_eq!(globals.locale().get().as_deref(), Some("es"));
    assert_eq!(globals.control_size().get(), Some(ControlSize::Small));
}

#[test]
fn a_canvas_follows_the_globals() {
    crate::reset_layout_runtime();
    let globals = Globals::new();
    let canvas = globals.canvas(Size::new(100.0, 100.0), empty).unwrap();
    assert_eq!(canvas.direction(), None);

    globals.mode().set(Some("dark".to_string()));
    globals.direction().set(Some(Direction::Rtl));
    globals.locale().set(Some("ar".to_string()));
    globals.control_size().set(Some(ControlSize::Large));
    globals.high_contrast().set(Some(true));
    globals.viewport().set(Some(Size::new(390.0, 844.0)));
    globals.safe_area().set(Insets::new(47.0, 0.0, 34.0, 0.0));
    assert_eq!(canvas.mode().as_deref(), Some("dark"));
    assert_eq!(canvas.direction(), Some(Direction::Rtl));
    assert_eq!(canvas.locale().as_deref(), Some("ar"));
    assert_eq!(canvas.control_size(), Some(ControlSize::Large));
    assert_eq!(canvas.high_contrast(), Some(true));
    assert_eq!(canvas.size(), Size::new(390.0, 844.0));
    assert_eq!(canvas.safe_area(), Insets::new(47.0, 0.0, 34.0, 0.0));

    globals.mode().set(None);
    globals.direction().set(None);
    globals.high_contrast().set(None);
    assert_eq!(canvas.direction(), None);
    assert_eq!(canvas.mode(), None);
    assert_eq!(canvas.high_contrast(), None);
}

#[test]
fn a_canvas_is_built_in_the_globals_it_starts_with() {
    crate::reset_layout_runtime();
    let globals = Globals::seeded(&PreviewEnv::NONE.direction(Direction::Rtl).locale("ar"));
    globals.safe_area().set(Insets::new(47.0, 0.0, 34.0, 0.0));
    let built_in = Rc::new(Cell::new(None));
    let seen = Rc::clone(&built_in);
    let _canvas = globals
        .canvas(Size::new(100.0, 100.0), move || {
            seen.set(Some((
                crate::current_direction(),
                crate::current_locale(),
                crate::use_safe_area_insets(),
            )));
            empty()
        })
        .unwrap();
    assert_eq!(
        built_in.take(),
        Some((
            Direction::Rtl,
            Some(String::from("ar")),
            Insets::new(47.0, 0.0, 34.0, 0.0)
        ))
    );
}

fn empty() -> Result<Box<dyn LayoutItem>, LayoutError> {
    Ok(Box::new(Container::new(LayoutStyle::new(), Vec::new())?))
}
