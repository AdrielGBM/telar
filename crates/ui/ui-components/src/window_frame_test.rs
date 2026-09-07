use super::*;
use crate::harness::press;
use layout_core::AvailableSpace;
use platform_core::{Event, PointerSource};
use ui_core::{Component, compute_layout};

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

/// The grip's whole job is arithmetic, and every part of it is invisible until it is wrong.
///
/// `on_drag` reports a position *local to the grip*, so a grip that forgot to add its own origin back would resize the window to about 14×14 the moment it was touched. And the grab offset — the distance from the pointer to the corner when the drag began — is what stops the corner teleporting to the cursor on the first event: press the middle of the grip and the window must not change size at all.
#[test]
fn the_grip_resizes_by_the_distance_dragged_not_to_the_pointer() {
    use std::cell::RefCell;
    crate::test_support::fresh_layout_runtime();

    let asked: Rc<RefCell<Vec<(f32, f32)>>> = Rc::new(RefCell::new(Vec::new()));
    let sink = asked.clone();
    let style = SurfaceFrameStyle {
        background: Color::TRANSPARENT,
        title_bar: Color::TRANSPARENT,
        title_text: Color::TRANSPARENT,
        close: Color::TRANSPARENT,
        radius: 0.0,
        font_size: 12.0,
        controls: WindowControls::default(),
        body_inset: 12.0,
        control_hover: Color::TRANSPARENT,
        close_hover: Color::TRANSPARENT,
    };
    let mut frame = window_frame(
        "Settings",
        None,
        style,
        Rc::new(|| {}),
        panel(),
        Some(Rc::new(move |w, h| sink.borrow_mut().push((w, h)))),
    )
    .unwrap();
    compute_layout(
        frame.layout_node(),
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(300.0),
    )
    .unwrap();

    let grip = Rect {
        x: 400.0 - 4.0 - GRIP_SIZE,
        y: 300.0 - 4.0 - GRIP_SIZE,
        width: GRIP_SIZE,
        height: GRIP_SIZE,
    };
    let (start_x, start_y) = (grip.x + GRIP_SIZE / 2.0, grip.y + GRIP_SIZE / 2.0);
    frame.on_event(&press(start_x as f64, start_y as f64));
    frame.on_event(&Event::PointerMoved {
        x: (start_x + 60.0) as f64,
        y: (start_y + 40.0) as f64,
        source: PointerSource::Mouse,
    });

    let asked = asked.borrow();
    assert_eq!(
        asked.first().copied(),
        Some((400.0, 300.0)),
        "grabbing the grip without moving must ask for the size the window already is"
    );
    assert_eq!(
        asked.last().copied(),
        Some((460.0, 340.0)),
        "the window grows by what the pointer travelled, not to where the pointer is"
    );
}

/// The grip has to be *on screen*, and a frame around an application is where it stops being.
///
/// A settings-sized float hands `surface_frame` a body sized to fill the window — its page area is a scroll leaf with a definite height, computed from the surface height less the chrome that existed before there was a grip. A body that will not shrink below its content pushes the grip row past the bottom edge, and the affordance builds, lays out, and is never visible. Which is exactly what happened.
#[test]
fn the_grip_stays_inside_a_window_whose_body_wants_all_of_it() {
    crate::test_support::fresh_layout_runtime();

    const SURFACE: (f32, f32) = (920.0, 680.0);
    let style = SurfaceFrameStyle {
        background: Color::TRANSPARENT,
        title_bar: Color::TRANSPARENT,
        title_text: Color::TRANSPARENT,
        close: Color::TRANSPARENT,
        radius: 0.0,
        font_size: 12.0,
        controls: WindowControls::default(),
        body_inset: 12.0,
        control_hover: Color::TRANSPARENT,
        close_hover: Color::TRANSPARENT,
    };
    let hungry = box_item(
        StyledContainer::new(
            LayoutStyle::new().width(600.0).height(SURFACE.1),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap(),
    );
    let asked: Rc<std::cell::RefCell<Vec<(f32, f32)>>> =
        Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink = asked.clone();
    let mut frame = window_frame(
        "Settings",
        None,
        style,
        Rc::new(|| {}),
        hungry,
        Some(Rc::new(move |w, h| sink.borrow_mut().push((w, h)))),
    )
    .unwrap();
    compute_layout(
        frame.layout_node(),
        AvailableSpace::Definite(SURFACE.0),
        AvailableSpace::Definite(SURFACE.1),
    )
    .unwrap();

    // A grip laid out past the bottom edge receives nothing, so nothing resizes.
    let (x, y) = (
        SURFACE.0 - 4.0 - GRIP_SIZE / 2.0,
        SURFACE.1 - 4.0 - GRIP_SIZE / 2.0,
    );
    frame.on_event(&press(x as f64, y as f64));
    frame.on_event(&Event::PointerMoved {
        x: (x + 40.0) as f64,
        y: (y + 30.0) as f64,
        source: PointerSource::Mouse,
    });

    let asked = asked.borrow();
    assert!(
        !asked.is_empty(),
        "nothing at the window's bottom-right corner answered a drag — a body that refuses to shrink \
         pushes the grip row off the surface, where it lays out perfectly and is never seen"
    );
    assert_eq!(
        asked.last().copied(),
        Some((SURFACE.0 + 40.0, SURFACE.1 + 30.0)),
        "and once it is on screen it still resizes by what the pointer travelled"
    );
}

#[test]
fn a_frame_without_a_resize_callback_draws_no_grip() {
    crate::test_support::fresh_layout_runtime();
    let style = SurfaceFrameStyle {
        background: Color::TRANSPARENT,
        title_bar: Color::TRANSPARENT,
        title_text: Color::TRANSPARENT,
        close: Color::TRANSPARENT,
        radius: 0.0,
        font_size: 12.0,
        controls: WindowControls::default(),
        body_inset: 12.0,
        control_hover: Color::TRANSPARENT,
        close_hover: Color::TRANSPARENT,
    };
    // A grip a backend cannot act on must be absent rather than present and inert.
    assert!(
        window_frame("Clock", None, style, Rc::new(|| {}), panel(), None).is_ok(),
        "a frame with no resize callback still builds"
    );
}

/// A leading item takes room in the strip without pushing the title out of it, and a zero body inset gives the body the full width below. Both exist so an application window — an icon beside its title, content owning everything under the strip — is this frame rather than a second one.
#[test]
fn a_leading_item_and_a_zero_inset_reshape_the_frame_without_replacing_it() {
    let filling = || {
        box_item(
            StyledContainer::new(
                LayoutStyle::new()
                    .width(SizeDimension::Percent(1.0))
                    .height(40.0),
                |_r| RectStyle::default(),
                vec![],
            )
            .unwrap(),
        )
    };
    // Everything is built inside the closure: `fresh_layout_runtime` empties the tree, so a widget made before it names a node that no longer exists — and, once ids are handed out again, someone else's.
    let laid_out = |inset: f32, with_icon: bool| {
        crate::test_support::fresh_layout_runtime();
        let body = filling();
        let body_rect = ui_core::track_layout(body.layout_node()).unwrap();
        let leading: Option<Box<dyn LayoutItem>> = with_icon.then(|| {
            box_item(
                StyledContainer::new(
                    LayoutStyle::new().width(16.0).height(16.0),
                    |_r| RectStyle::default(),
                    vec![],
                )
                .unwrap(),
            )
        });
        let leading_rect = leading
            .as_ref()
            .and_then(|item| ui_core::track_layout(item.layout_node()));
        let style = SurfaceFrameStyle {
            body_inset: inset,
            ..SurfaceFrameStyle::default()
        };
        let frame = window_frame("Files", leading, style, Rc::new(|| {}), body, None).unwrap();
        compute_layout(
            frame.layout_node(),
            AvailableSpace::Definite(300.0),
            AvailableSpace::Definite(200.0),
        )
        .unwrap();
        (body_rect.get(), leading_rect.map(|r| r.get()))
    };

    let (flush, icon_rect) = laid_out(0.0, true);
    assert_eq!(
        icon_rect.map(|r| r.width),
        Some(16.0),
        "the leading item was not laid out"
    );
    assert_eq!(flush.x, 0.0, "a zero inset must leave the body flush");
    assert_eq!(flush.width, 300.0, "and give it the whole width");

    let (inset, _) = laid_out(12.0, false);
    assert_eq!(
        inset.x, 12.0,
        "the default inset must still hold the body off the edge"
    );
}
