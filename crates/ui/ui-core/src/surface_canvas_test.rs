use std::cell::Cell;
use std::rc::Rc;

use geometry_core::{Insets, Rect, Size, Transform};
use layout_core::{LayoutStyle, SizeDimension};
use platform_core::{Event, PointerButton, PointerSource};
use reactive_core::{dispose_owner, effect, owner_scope, signal};
use renderer_core::{Color, RectStyle, ShapeStyle};
use ui_tree::{EventResult, RenderNode};

use super::*;
use crate::{Container, Overlay, StyledContainer, reset_layout_runtime};

fn press(x: f64, y: f64) -> Event {
    Event::PointerPressed {
        x,
        y,
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    }
}

fn release(x: f64, y: f64) -> Event {
    Event::PointerReleased {
        x,
        y,
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    }
}

fn fill() -> LayoutStyle {
    LayoutStyle::new()
        .width(SizeDimension::Percent(1.0))
        .height(SizeDimension::Percent(1.0))
}

fn counting_scrim(hits: Rc<Cell<u32>>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let scrim = Container::new(fill(), vec![])?.on_press(move || hits.set(hits.get() + 1));
    Ok(Box::new(Overlay::new(
        LayoutStyle::new(),
        vec![Box::new(scrim)],
    )?))
}

/// Overlay ids are minted per surface, so the canvas's first overlay and the page's share an id. Content owned by the scope that built the canvas was torn down when that scope was, with the page's world active, and withdrew the page's overlay in place of its own.
#[test]
fn dropping_a_host_withdraws_its_overlays_from_its_own_surface_only() {
    reset_layout_runtime();
    let page_hits = Rc::new(Cell::new(0));
    let page_overlay = counting_scrim(page_hits.clone()).unwrap();
    crate::compute_layout(
        page_overlay.layout_node(),
        layout_core::AvailableSpace::Definite(100.0),
        layout_core::AvailableSpace::Definite(100.0),
    )
    .unwrap();

    let component = owner_scope();
    let canvas = SurfaceCanvas::new(Size::new(100.0, 100.0), || {
        counting_scrim(Rc::new(Cell::new(0)))
    })
    .unwrap();
    let component_id = component.id();
    drop(component);
    drop(canvas);
    dispose_owner(component_id);

    crate::dispatch_overlays(&press(10.0, 10.0));
    crate::dispatch_overlays(&release(10.0, 10.0));
    assert_eq!(page_hits.get(), 1, "the page's overlay is still registered");
}

#[test]
fn dispatch_maps_the_pointer_back_through_the_placement() {
    reset_layout_runtime();
    let pressed = signal(0u32);
    let canvas = SurfaceCanvas::new(Size::new(100.0, 100.0), move || {
        let target = StyledContainer::new(
            LayoutStyle::new().width(20.0).height(20.0),
            |_| RectStyle::default().with_fill(Color::BLACK),
            vec![],
        )?
        .on_press(move || pressed.set(pressed.get() + 1));
        Ok(Box::new(Container::new(fill(), vec![Box::new(target)])?) as Box<dyn LayoutItem>)
    })
    .unwrap();
    canvas.resize(Size::new(100.0, 100.0));
    let _ = canvas.frame_commands();
    canvas.set_placement(Transform {
        a: 2.0,
        d: 2.0,
        e: 100.0,
        f: 100.0,
        ..Transform::IDENTITY
    });

    canvas.dispatch(&press(30.0, 30.0));
    canvas.dispatch(&release(30.0, 30.0));
    assert_eq!(
        pressed.get(),
        0,
        "the outer point (30, 30) is outside the placed surface"
    );

    canvas.dispatch(&press(130.0, 130.0));
    canvas.dispatch(&release(130.0, 130.0));
    assert_eq!(
        pressed.get(),
        1,
        "(130, 130) lands on the surface's (15, 15)"
    );
}

#[test]
fn content_lives_as_long_as_the_host_and_not_the_scope_it_was_built_in() {
    reset_layout_runtime();
    let source = signal(0u32);
    let seen = Rc::new(Cell::new(0u32));
    let caller = owner_scope();
    let canvas = {
        let seen = seen.clone();
        SurfaceCanvas::new(Size::new(10.0, 10.0), move || {
            effect(move || seen.set(source.get()));
            Ok(Box::new(Container::new(fill(), vec![])?) as Box<dyn LayoutItem>)
        })
        .unwrap()
    };
    let caller_id = caller.id();
    drop(caller);
    dispose_owner(caller_id);

    source.set(3);
    assert_eq!(
        seen.get(),
        3,
        "disposing the caller's scope left the content running"
    );

    drop(canvas);
    source.set(4);
    assert_eq!(seen.get(), 3, "dropping the canvas stopped it");
}

#[test]
fn a_composited_surface_is_clipped_to_where_it_lands() {
    let clip_of = |node: RenderNode| match node {
        RenderNode::Clip { rect, .. } => rect,
        _ => panic!("a composited surface is a clip"),
    };
    let size = Size::new(100.0, 50.0);
    let zoomed = Transform {
        a: 0.5,
        d: 0.5,
        e: 10.0,
        f: 20.0,
        ..Transform::IDENTITY
    };
    assert_eq!(
        clip_of(composite_surface(zoomed, size, [])),
        Rect::new(10.0, 20.0, 50.0, 25.0)
    );
    let mirrored = Transform {
        a: -1.0,
        e: 200.0,
        ..Transform::IDENTITY
    };
    assert_eq!(
        clip_of(composite_surface(mirrored, size, [])),
        Rect::new(100.0, 0.0, 100.0, 50.0)
    );
}

#[test]
fn a_host_routes_overlays_before_its_tree() {
    reset_layout_runtime();
    let behind = signal(0u32);
    let modal_hits = Rc::new(Cell::new(0));
    let canvas = {
        let modal_hits = modal_hits.clone();
        SurfaceCanvas::new(Size::new(100.0, 100.0), move || {
            let page = Container::new(fill(), vec![])?.on_press(move || behind.set(1));
            let modal = counting_scrim(modal_hits)?;
            Ok(
                Box::new(Container::new(fill(), vec![Box::new(page), modal])?)
                    as Box<dyn LayoutItem>,
            )
        })
        .unwrap()
    };
    canvas.resize(Size::new(100.0, 100.0));

    assert_eq!(canvas.dispatch(&press(50.0, 50.0)), EventResult::Handled);
    canvas.dispatch(&release(50.0, 50.0));
    assert_eq!(modal_hits.get(), 1);
    assert_eq!(
        behind.get(),
        0,
        "the modal kept the press from the page behind it"
    );
}

#[test]
fn a_canvas_in_an_environment_is_built_in_it() {
    reset_layout_runtime();
    let env = SurfaceEnv {
        mode: Some(String::from("dusk")),
        locale: Some(String::from("ar")),
        direction: Some(Direction::Rtl),
        control_size: Some(ControlSize::Large),
        high_contrast: Some(true),
        safe_area: Insets::new(44.0, 0.0, 34.0, 0.0),
    };
    let built_in = Rc::new(Cell::new(None));
    let seen = Rc::clone(&built_in);
    let canvas = SurfaceCanvas::new_in(Size::new(10.0, 10.0), &env, move || {
        seen.set(Some(SurfaceEnv {
            mode: theme_core::use_mode(),
            locale: i18n_core::current_locale(),
            direction: Some(crate::current_direction()),
            control_size: Some(theme_core::current_control_size()),
            high_contrast: preferences_core::high_contrast(),
            safe_area: crate::use_safe_area_insets(),
        }));
        Ok(Box::new(Container::new(fill(), vec![])?) as Box<dyn LayoutItem>)
    })
    .unwrap();
    assert_eq!(built_in.take(), Some(env));
    assert_eq!(canvas.direction(), Some(Direction::Rtl));
    assert_eq!(canvas.safe_area(), Insets::new(44.0, 0.0, 34.0, 0.0));
}
