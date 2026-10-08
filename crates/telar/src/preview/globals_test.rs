use super::*;
use crate::{Container, LayoutItem, LayoutStyle};

#[test]
fn new_globals_set_nothing() {
    let globals = Globals::new();
    assert_eq!(globals.mode().get(), None);
    assert_eq!(globals.locale().get(), None);
    assert_eq!(globals.direction().get(), None);
    assert_eq!(globals.viewport().get(), None);
    assert_eq!(globals.background().get(), None);
    assert_eq!(globals.control_size().get(), None);
}

#[test]
fn a_preview_env_seeds_what_it_names() {
    let env = PreviewEnv::NONE
        .mode("dark")
        .locale("ar")
        .direction(Direction::Rtl)
        .viewport(390.0, 844.0)
        .background(Color::rgba(0.0, 0.0, 0.0, 1.0));
    let globals = Globals::seeded(&env);
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
fn a_bound_canvas_follows_the_globals() {
    crate::reset_layout_runtime();
    let canvas = Rc::new(
        SurfaceCanvas::new(Size::new(100.0, 100.0), || {
            Ok(Box::new(Container::new(LayoutStyle::new(), Vec::new())?) as Box<dyn LayoutItem>)
        })
        .unwrap(),
    );
    let globals = Globals::new();
    globals.bind(&canvas);
    assert_eq!(canvas.direction(), None);

    globals.direction().set(Some(Direction::Rtl));
    globals.locale().set(Some("ar".to_string()));
    globals.control_size().set(Some(ControlSize::Large));
    globals.viewport().set(Some(Size::new(390.0, 844.0)));
    assert_eq!(canvas.direction(), Some(Direction::Rtl));
    assert_eq!(canvas.locale().as_deref(), Some("ar"));
    assert_eq!(canvas.control_size(), Some(ControlSize::Large));
    assert_eq!(canvas.size(), Size::new(390.0, 844.0));

    globals.direction().set(None);
    assert_eq!(canvas.direction(), None);
}
