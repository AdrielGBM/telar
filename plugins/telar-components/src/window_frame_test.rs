use std::cell::Cell;

use super::*;
use telar::testing::press;
use telar::{AvailableSpace, Component, Event, PointerSource, compute_layout};

fn panel() -> Children {
    Children::one(|| {
        Ok(box_item(StyledContainer::new(
            LayoutStyle::new().width(100.0).height(40.0),
            |_r| RectStyle::default(),
            vec![],
        )?))
    })
}

/// A frame titled `title` around [`panel`], painted as `style`, whose close does nothing and that draws no grip.
fn framing(title: &'static str, style: SurfaceFrameStyle) -> WindowFrameProps {
    WindowFrameProps::props()
        .title(title)
        .style(style)
        .close(Rc::new(|| {}))
        .body(panel())
        .build()
}

/// The grip's whole job is arithmetic, and every part of it is invisible until it is wrong.
///
/// `on_drag` reports a position *local to the grip*, so a grip that forgot to add its own origin back would resize the window to about the size of the grip the moment it was touched. And the grab offset — the distance from the pointer to the corner when the drag began — is what stops the corner teleporting to the cursor on the first event: press the middle of the grip and the window must not change size at all.
#[test]
fn the_grip_resizes_by_the_distance_dragged_not_to_the_pointer() {
    use std::cell::RefCell;
    crate::test_support::fresh_layout_runtime();

    let asked: Rc<RefCell<Vec<(f32, f32)>>> = Rc::new(RefCell::new(Vec::new()));
    let sink = asked.clone();
    let style = SurfaceFrameStyle::default();
    let mut frame = window_frame(WindowFrameProps {
        resize: Some(Rc::new(move |w, h| sink.borrow_mut().push((w, h)))),
        ..framing("Settings", style)
    })
    .unwrap();
    compute_layout(
        frame.layout_node(),
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(300.0),
    )
    .unwrap();

    let (side, inset) = (crate::shared::icon_size(), crate::shared::gap_sm());
    let grip = Rect {
        x: 400.0 - inset - side,
        y: 300.0 - inset - side,
        width: side,
        height: side,
    };
    let (start_x, start_y) = (grip.x + side / 2.0, grip.y + side / 2.0);
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
    let style = SurfaceFrameStyle::default();
    let hungry = Children::one(|| {
        Ok(box_item(StyledContainer::new(
            LayoutStyle::new().width(600.0).height(SURFACE.1),
            |_r| RectStyle::default(),
            vec![],
        )?))
    });
    let asked: Rc<std::cell::RefCell<Vec<(f32, f32)>>> =
        Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink = asked.clone();
    let mut frame = window_frame(WindowFrameProps {
        body: hungry,
        resize: Some(Rc::new(move |w, h| sink.borrow_mut().push((w, h)))),
        ..framing("Settings", style)
    })
    .unwrap();
    compute_layout(
        frame.layout_node(),
        AvailableSpace::Definite(SURFACE.0),
        AvailableSpace::Definite(SURFACE.1),
    )
    .unwrap();

    // A grip laid out past the bottom edge receives nothing, so nothing resizes.
    let reach = crate::shared::gap_sm() + crate::shared::icon_size() / 2.0;
    let (x, y) = (SURFACE.0 - reach, SURFACE.1 - reach);
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
    let style = SurfaceFrameStyle::default();
    // A grip a backend cannot act on must be absent rather than present and inert.
    assert!(
        window_frame(framing("Clock", style)).is_ok(),
        "a frame with no resize callback still builds"
    );
}

/// An empty box laid out as `style`, built where the frame places it, that records where it was laid out in `rect`.
fn recording(rect: &Rc<Cell<Option<RwSignal<Rect>>>>, style: fn() -> LayoutStyle) -> Children {
    let rect = rect.clone();
    Children::one(move || {
        let item = StyledContainer::new(style(), |_r| RectStyle::default(), vec![])?;
        rect.set(telar::track_layout(item.layout_node()));
        Ok(box_item(item))
    })
}

/// A leading item takes room in the strip without pushing the title out of it, and a zero body inset gives the body the full width below. Both exist so an application window — an icon beside its title, content owning everything under the strip — is this frame rather than a second one.
#[test]
fn a_leading_item_and_a_zero_inset_reshape_the_frame_without_replacing_it() {
    // Everything is built inside the closure: `fresh_layout_runtime` empties the tree, so a widget made before it names a node that no longer exists — and, once ids are handed out again, someone else's.
    let laid_out = |inset: f32, with_icon: bool| {
        crate::test_support::fresh_layout_runtime();
        let body_rect = Rc::new(Cell::new(None));
        let leading_rect = Rc::new(Cell::new(None));
        let body = recording(&body_rect, || {
            LayoutStyle::new()
                .width(SizeDimension::Percent(1.0))
                .height(40.0)
        });
        let leading = match with_icon {
            true => recording(&leading_rect, || {
                LayoutStyle::new().width(16.0).height(16.0)
            }),
            false => Children::default(),
        };
        let style = SurfaceFrameStyle {
            body_inset: Some(inset),
            ..SurfaceFrameStyle::default()
        };
        let frame = window_frame(WindowFrameProps {
            leading,
            body,
            ..framing("Files", style)
        })
        .unwrap();
        compute_layout(
            frame.layout_node(),
            AvailableSpace::Definite(300.0),
            AvailableSpace::Definite(200.0),
        )
        .unwrap();
        (
            body_rect.get().expect("the body was built").get(),
            leading_rect.get().map(|r: RwSignal<Rect>| r.get()),
        )
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

#[derive(Clone, telar::ThemeTokens)]
#[theme(default(
    on_primary,
    radius,
    spacing,
    icon_size,
    muted,
    scrollbar,
    ink,
    surface_alt,
    border,
    success,
    warning,
    error,
    on_error,
    info,
    highlight_low,
    highlight_med,
    highlight_high
))]
struct PaperTheme {
    primary: Color,
    surface: Color,
}

fn fills(commands: &[telar::DrawCommand]) -> Vec<telar::Paint> {
    commands
        .iter()
        .filter_map(|command| match command {
            telar::DrawCommand::Rect { style, .. } => style.fill,
            _ => None,
        })
        .collect()
}

#[test]
fn a_frame_restyles_on_a_mode_switch_without_being_rebuilt() {
    crate::test_support::fresh_layout_runtime();
    let light = Color::rgba(1.0, 1.0, 1.0, 1.0);
    let dark = Color::rgba(0.0, 0.0, 0.0, 1.0);
    telar::register_mode("frame-light", move || {
        telar::set_theme(PaperTheme {
            primary: light,
            surface: light,
        })
    });
    telar::register_mode("frame-dark", move || {
        telar::set_theme(PaperTheme {
            primary: dark,
            surface: dark,
        })
    });
    telar::set_mode("frame-light");

    let style = SurfaceFrameStyle::default()
        .background(Reactive::of(|| telar::use_theme_tokens().surface()));
    let frame = window_frame(framing("Files", style)).unwrap();
    compute_layout(
        frame.layout_node(),
        AvailableSpace::Definite(300.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();
    let tree = telar::ComponentList::new(frame);
    assert!(fills(&tree.commands()).contains(&light.into()));

    telar::set_mode("frame-dark");
    let after = fills(&tree.commands());
    assert!(after.contains(&dark.into()), "the card kept its old colour");
    assert!(!after.contains(&light.into()));
}

fn mark_color(tree: &telar::ComponentList) -> telar::Paint {
    tree.commands()
        .iter()
        .find_map(|command| match command {
            telar::DrawCommand::Path { style, .. } => style.stroke.as_ref().map(|s| s.paint),
            _ => None,
        })
        .expect("the mark is drawn")
}

use telar::testing::centre;

fn mark_centre(tree: &telar::ComponentList) -> (f64, f64) {
    let commands = tree.commands();
    let at = commands
        .iter()
        .position(|command| matches!(command, telar::DrawCommand::Path { .. }))
        .expect("the mark is drawn");
    let (telar::DrawCommand::Path { data, .. }, telar::DrawCommand::PushMatrix { matrix }) =
        (&commands[at], &commands[at - 1])
    else {
        panic!("the mark is not drawn under its own translation");
    };
    let (x, y) = centre(data.bounds().expect("the mark has extent"));
    (x + matrix[4] as f64, y + matrix[5] as f64)
}

#[test]
fn the_close_mark_swaps_to_its_hover_colour_while_the_pointer_is_over_it() {
    use telar::testing::{moved, route};

    crate::test_support::fresh_layout_runtime();
    let resting = Color::rgba(0.2, 0.2, 0.2, 1.0);
    let hovered = Color::rgba(1.0, 1.0, 1.0, 1.0);
    let style = SurfaceFrameStyle::default()
        .close(resting)
        .close_hover(Color::rgba(0.8, 0.1, 0.1, 1.0))
        .close_hover_glyph(hovered);
    let frame = window_frame(framing("Files", style)).unwrap();
    compute_layout(
        frame.layout_node(),
        AvailableSpace::Definite(300.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();
    telar::relayout_if_dirty();
    let mut tree = telar::ComponentList::new(frame);
    assert_eq!(mark_color(&tree), resting.into());

    let (x, y) = mark_centre(&tree);
    route(&mut tree, &moved(x, y));
    assert_eq!(mark_color(&tree), hovered.into());

    route(&mut tree, &moved(0.0, 190.0));
    assert_eq!(mark_color(&tree), resting.into());
}

fn laid_out_frame(style: SurfaceFrameStyle) -> telar::ComponentList {
    crate::test_support::fresh_layout_runtime();
    let frame = window_frame(framing("Files", style)).unwrap();
    compute_layout(
        frame.layout_node(),
        AvailableSpace::Definite(300.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();
    telar::relayout_if_dirty();
    telar::ComponentList::new(frame)
}

/// A frame that names no colours is drawn in the theme's own: a card in `surface`, a strip in `surface_alt`, square, so a backend gets chrome that matches the catalogue without restating the theme.
#[test]
fn an_unstyled_frame_paints_from_the_theme_roles() {
    let theme = PaperTheme {
        primary: Color::rgba(0.2, 0.3, 0.9, 1.0),
        surface: Color::rgba(0.95, 0.94, 0.9, 1.0),
    };
    telar::set_theme(theme.clone());
    let tree = laid_out_frame(SurfaceFrameStyle::default());
    let card = crate::test_support::first_painted(&tree);
    assert_eq!(card.fill, Some(theme.surface.into()));
    assert_eq!(
        card.radius,
        telar::BorderRadius::all(0.0),
        "square unless asked"
    );
    assert!(fills(&tree.commands()).contains(&telar::ThemeTokens::surface_alt(&theme).into()));
}

#[test]
fn a_named_radius_and_a_card_amendment_reshape_the_card() {
    let ink = Color::rgba(0.1, 0.1, 0.1, 0.8);
    let style = SurfaceFrameStyle {
        radius: Some(9.0),
        ..SurfaceFrameStyle::default()
    }
    .card(Rc::new(move |paint: RectStyle| paint.with_fill(ink)));
    let card = crate::test_support::first_painted(&laid_out_frame(style));
    assert_eq!(card.fill, Some(ink.into()));
    assert_eq!(card.radius, telar::BorderRadius::all(9.0));
}

fn framed_window(controls: WindowControls) -> telar::ComponentList {
    let style = SurfaceFrameStyle {
        controls,
        ..SurfaceFrameStyle::default()
    };
    let tree = laid_out_frame(style);
    telar::take_window_commands();
    tree
}

fn window_asked() -> Vec<telar::WindowCommand> {
    telar::take_window_commands()
        .into_iter()
        .filter(|command| !matches!(command, telar::WindowCommand::SetCursor(_)))
        .collect()
}

/// A point of the title strip between the title and the controls.
const STRIP: (f64, f64) = (150.0, 12.0);

fn click(tree: &mut telar::ComponentList, (x, y): (f64, f64)) {
    use telar::testing::{release, route};
    route(tree, &press(x, y));
    route(tree, &release(x, y));
}

#[test]
fn the_strip_of_a_window_frame_moves_and_maximizes_the_window_it_is_the_title_bar_of() {
    use telar::testing::{moved, route};

    let mut tree = framed_window(WindowControls {
        drag: true,
        minimize: true,
        maximize: true,
    });
    click(&mut tree, STRIP);
    click(&mut tree, STRIP);
    assert_eq!(window_asked(), [telar::WindowCommand::ToggleMaximize]);

    let (x, y) = STRIP;
    route(&mut tree, &press(x, y));
    route(&mut tree, &moved(x + 20.0, y));
    assert_eq!(window_asked(), [telar::WindowCommand::Drag]);
    assert!(
        !telar::take_window_commands()
            .iter()
            .any(|command| matches!(command, telar::WindowCommand::SetDecorations(_))),
        "a frame leaves the window's decorations to the backend that opened it"
    );
}

/// A strip that does not move the window is not the window's title bar, and neither does a double-click on it maximize the window, nor a window whose frame offers no maximize.
#[test]
fn a_strip_that_is_not_the_window_s_title_bar_leaves_the_window_as_it_is() {
    use telar::testing::{moved, route};

    for controls in [
        WindowControls {
            drag: false,
            minimize: true,
            maximize: true,
        },
        WindowControls {
            drag: true,
            minimize: true,
            maximize: false,
        },
    ] {
        let mut tree = framed_window(controls);
        click(&mut tree, STRIP);
        click(&mut tree, STRIP);
        assert!(window_asked().is_empty(), "{controls:?}");
        if !controls.drag {
            let (x, y) = STRIP;
            route(&mut tree, &press(x, y));
            route(&mut tree, &moved(x + 20.0, y));
            assert!(window_asked().is_empty(), "{controls:?}");
        }
    }
}
