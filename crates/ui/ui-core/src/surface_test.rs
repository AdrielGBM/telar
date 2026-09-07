use super::*;
use renderer_core::TextStyle;
use std::cell::Cell;

use platform_core::PointerSource;
use renderer_core::RectStyle;

use crate::StyledContainer;
use crate::context::reset_layout_runtime;
use crate::layout_item::box_item;

fn panel() -> Box<dyn LayoutItem> {
    box_item(
        StyledContainer::new(
            LayoutStyle::new().width(100.0).height(40.0),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap(),
    )
}

fn press(x: f64, y: f64) -> Event {
    Event::PointerPressed {
        x,
        y,
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    }
}

#[test]
fn dismiss_fires_on_outside_press_only() {
    reset_layout_runtime();
    let fired = Rc::new(Cell::new(0u32));
    let f = fired.clone();
    let mut scaffold = SurfaceScaffold::new(
        Edge::Top,
        AlignItems::CENTER,
        (20, 20, 20, 20),
        Some(DEFAULT_SCRIM),
        Some(Rc::new(move || f.set(f.get() + 1))),
        panel(),
    )
    .unwrap();
    scaffold.on_event(&Event::WindowResized {
        width: 200,
        height: 200,
    });

    scaffold.on_event(&press(100.0, 40.0));
    assert_eq!(fired.get(), 0, "a press inside the panel must not dismiss");
    scaffold.on_event(&press(10.0, 190.0));
    assert_eq!(fired.get(), 1, "a press outside the panel dismisses");
}

/// Escape is the keyboard's way out of a surface that a press outside would also close, and it only reaches the surface when nothing inside wanted it: a focused field, an armed confirmation and an open dropdown all cancel themselves first, and taking the whole surface down with them is the bug this guards.
#[test]
fn escape_dismisses_only_what_the_panel_left_alone() {
    reset_layout_runtime();
    let fired = Rc::new(Cell::new(0u32));
    let f = fired.clone();
    let mut scaffold = SurfaceScaffold::new(
        Edge::Top,
        AlignItems::CENTER,
        (20, 20, 20, 20),
        Some(DEFAULT_SCRIM),
        Some(Rc::new(move || f.set(f.get() + 1))),
        panel(),
    )
    .unwrap();
    scaffold.on_event(&Event::WindowResized {
        width: 200,
        height: 200,
    });

    let escape = Event::KeyPressed {
        key: Key::Named(NamedKey::Escape),
        modifiers: Default::default(),
    };
    assert_eq!(scaffold.on_event(&escape), EventResult::Handled);
    assert_eq!(
        fired.get(),
        1,
        "a plain panel lets Escape close the surface"
    );

    reset_layout_runtime();
    let untouched = Rc::new(Cell::new(0u32));
    let u = untouched.clone();
    let field = crate::input::Input::new(
        reactive_core::signal(String::new()),
        LayoutStyle::new().width(100.0).height(30.0),
        || TextStyle::new(14.0, Color::BLACK),
    )
    .unwrap()
    .autofocus();
    let mut focused = SurfaceScaffold::new(
        Edge::Top,
        AlignItems::CENTER,
        (20, 20, 20, 20),
        Some(DEFAULT_SCRIM),
        Some(Rc::new(move || u.set(u.get() + 1))),
        box_item(field),
    )
    .unwrap();
    focused.on_event(&Event::WindowResized {
        width: 200,
        height: 200,
    });
    focused.on_event(&press(100.0, 40.0));
    focused.on_event(&escape);
    assert_eq!(
        untouched.get(),
        0,
        "a field that claims Escape to release its own focus keeps the surface up"
    );
}

#[test]
fn cross_axis_margin_insets_the_panel_from_the_side_edges() {
    reset_layout_runtime();
    let fired = Rc::new(Cell::new(0u32));
    let f = fired.clone();
    let mut scaffold = SurfaceScaffold::new(
        Edge::Top,
        AlignItems::START,
        (30, 10, 10, 10),
        Some(DEFAULT_SCRIM),
        Some(Rc::new(move || f.set(f.get() + 1))),
        panel(),
    )
    .unwrap();
    scaffold.on_event(&Event::WindowResized {
        width: 200,
        height: 200,
    });

    scaffold.on_event(&press(50.0, 40.0));
    assert_eq!(
        fired.get(),
        0,
        "a press on the inset panel must not dismiss"
    );
    scaffold.on_event(&press(4.0, 40.0));
    assert_eq!(
        fired.get(),
        1,
        "a press in the left gap (before the inset) dismisses"
    );
    scaffold.on_event(&press(150.0, 40.0));
    assert_eq!(fired.get(), 2, "a press in the right gap dismisses");
}

#[test]
fn no_dismiss_when_not_configured() {
    reset_layout_runtime();
    let fired = Rc::new(Cell::new(0u32));
    let f = fired.clone();
    let mut scaffold = SurfaceScaffold::new(
        Edge::Top,
        AlignItems::CENTER,
        (0, 0, 0, 0),
        Some(DEFAULT_SCRIM),
        None,
        panel(),
    )
    .unwrap();
    let _ = &f;
    scaffold.on_event(&Event::WindowResized {
        width: 200,
        height: 200,
    });

    scaffold.on_event(&press(10.0, 190.0));
    assert_eq!(
        fired.get(),
        0,
        "no dismiss must fire when dismiss_on_outside is off"
    );
}
#[test]
fn enter_transform_fade_is_opacity_only() {
    assert_eq!(enter_transform(EnterMotion::Fade, 0.0), (IDENTITY, 0.0));
    assert_eq!(enter_transform(EnterMotion::Fade, 0.5), (IDENTITY, 0.5));
    assert_eq!(enter_transform(EnterMotion::Fade, 1.0), (IDENTITY, 1.0));
}

#[test]
fn enter_transform_slide_offsets_from_edge_then_settles() {
    let (m, o) = enter_transform(EnterMotion::Slide(Edge::Top), 0.0);
    assert_eq!(o, 0.0);
    assert_eq!(m[5], -SLIDE_DISTANCE, "top slides down from above");
    assert_eq!(
        enter_transform(EnterMotion::Slide(Edge::Top), 1.0),
        (IDENTITY, 1.0)
    );
    assert_eq!(
        enter_transform(EnterMotion::Slide(Edge::Bottom), 0.0).0[5],
        SLIDE_DISTANCE
    );
    assert_eq!(
        enter_transform(EnterMotion::Slide(Edge::Left), 0.0).0[4],
        -SLIDE_DISTANCE
    );
    assert_eq!(
        enter_transform(EnterMotion::Slide(Edge::Right), 0.0).0[4],
        SLIDE_DISTANCE
    );
}
