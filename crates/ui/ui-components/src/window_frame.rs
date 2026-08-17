//! Titled, closable window chrome with an optional resize grip.
//!
//! It lived in `ui-core` while that crate was the only place a full-surface root could be assembled, and it
//! never belonged there: it is `Text` plus two `StyledContainer`s plus a closure over `track_layout` — a
//! composed widget using nothing the primitive layer has that the catalogue lacks. "Titled closable window
//! chrome" is a catalogue entry, not a layout primitive.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use geometry_core::Rect;
use layout_core::{AlignItems, JustifyContent, LayoutError, LayoutStyle, SizeDimension};
use reactive_core::RwSignal;
use renderer_core::{Color, RectStyle, TextStyle};
use ui_core::{LayoutItem, StyledContainer, Text, box_item, track_layout};

#[derive(Debug, Clone, Copy)]
pub struct SurfaceFrameStyle {
    pub background: Color,
    pub title_bar: Color,
    pub title_text: Color,
    pub close: Color,
    pub radius: f32,
    pub font_size: f32,
}

/// The smallest a frame will ask to become. A window dragged to nothing is a window the user cannot get hold of
/// again — its own grip goes with it.
pub const MIN_FRAME_SIZE: (f32, f32) = (180.0, 120.0);

/// The corner grip's side, in logical pixels. Big enough to hit without aiming, small enough not to read as
/// content.
const GRIP_SIZE: f32 = 14.0;

/// The rect a grip measures the frame against. It is a cell rather than a signal because the grip has to exist
/// before the row that holds it, and that row before the card that holds *both* — so the one rect the grip needs
/// is the one thing it cannot be handed at construction. Filled in as soon as the card exists.
type DeferredRect = Rc<RefCell<Option<RwSignal<Rect>>>>;

/// A resize grip for the bottom-right corner of a frame, reporting the size the *surface* should become.
///
/// The arithmetic is the whole of it. `on_drag` reports where the pointer is **inside the grip**, so the grip's
/// own laid-out origin has to be added back to reach surface space — and then the grab offset, the distance from
/// the pointer to the corner when the drag began, has to come off it, or the corner jumps to the cursor the
/// instant it is touched. The offset is latched once per drag rather than recomputed, because the card it was
/// measured against is resizing underneath the gesture.
fn resize_grip(
    color: Color,
    card_rect: DeferredRect,
    resize: Rc<dyn Fn(f32, f32)>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let grip = StyledContainer::new(
        LayoutStyle::new().width(GRIP_SIZE).height(GRIP_SIZE),
        move |_| RectStyle::filled(color, 2.0),
        vec![],
    )?;
    let grip_rect = track_layout(grip.layout_node());
    let grab: Rc<Cell<Option<(f32, f32)>>> = Rc::new(Cell::new(None));
    let release = Rc::clone(&grab);
    Ok(box_item(
        grip.on_drag(move |local_x, local_y| {
            let (Some(grip_rect), Some(card_rect)) = (&grip_rect, card_rect.borrow().clone())
            else {
                return;
            };
            let (grip, card) = (grip_rect.get(), card_rect.get());
            let (x, y) = (grip.x + local_x, grip.y + local_y);
            let (offset_x, offset_y) = match grab.get() {
                Some(offset) => offset,
                None => {
                    let offset = (x - (card.x + card.width), y - (card.y + card.height));
                    grab.set(Some(offset));
                    offset
                }
            };
            resize(
                (x - offset_x - card.x).max(MIN_FRAME_SIZE.0),
                (y - offset_y - card.y).max(MIN_FRAME_SIZE.1),
            );
        })
        .on_drag_end(move |_, _| release.set(None)),
    ))
}

/// A titled, closable window frame around `body`.
///
/// `resize` opts the frame into a corner grip: it is handed the size the surface should take, in logical
/// pixels, on every move of that grip. A backend that can renegotiate a surface's size wires it up; one that
/// cannot passes `None` and the grip is not drawn, rather than drawn and inert.
pub fn window_frame(
    title: impl Into<String>,
    style: SurfaceFrameStyle,
    close: std::rc::Rc<dyn Fn()>,
    body: Box<dyn LayoutItem>,
    resize: Option<Rc<dyn Fn(f32, f32)>>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let title = title.into();
    let title_color = style.title_text;
    let font_size = style.font_size;
    let title_label = box_item(Text::auto(
        move || title.clone(),
        LayoutStyle::new(),
        move || TextStyle::new(font_size, title_color),
    )?);

    let close_color = style.close;
    let close_label = box_item(Text::auto(
        || "\u{2715}".to_string(),
        LayoutStyle::new(),
        move || TextStyle::new(font_size, close_color),
    )?);
    let close_button = box_item(
        StyledContainer::new(
            LayoutStyle::new()
                .align_items(AlignItems::CENTER)
                .justify_content(JustifyContent::CENTER)
                .padding_horizontal(8.0)
                .padding_vertical(2.0),
            |_| RectStyle::default(),
            vec![close_label],
        )?
        .on_press(move || close()),
    );

    let title_bar_color = style.title_bar;
    let title_bar = box_item(StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::SPACE_BETWEEN)
            .width(SizeDimension::Percent(1.0))
            .padding_horizontal(12.0)
            .padding_vertical(8.0),
        move |_| RectStyle::filled(title_bar_color, 0.0),
        vec![title_label, close_button],
    )?);

    // A flex item may not shrink below its content unless you say so, and an application body sized to fill the window (the settings page area is a scroll leaf with a definite height) otherwise refuses to give up a single pixel and pushes the grip row off the bottom of the surface — a resize affordance that exists, lays out, and is never on screen.
    let body_area = box_item(StyledContainer::new(
        LayoutStyle::new()
            .flex_column()
            .flex_grow(1.0)
            .min_height(0.0)
            .width(SizeDimension::Percent(1.0))
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER)
            .padding_all(12.0),
        |_| RectStyle::default(),
        vec![body],
    )?);

    let card_rect: DeferredRect = Rc::new(RefCell::new(None));
    let mut children = vec![title_bar, body_area];
    if let Some(resize) = resize {
        children.push(box_item(StyledContainer::new(
            LayoutStyle::new()
                .flex_row()
                .width(SizeDimension::Percent(1.0))
                .flex_shrink(0.0)
                .justify_content(JustifyContent::END)
                .padding_horizontal(4.0)
                .padding_bottom(4.0),
            |_| RectStyle::default(),
            vec![resize_grip(style.close, Rc::clone(&card_rect), resize)?],
        )?));
    }

    let background = style.background;
    let radius = style.radius;
    let card = StyledContainer::new(
        LayoutStyle::new()
            .flex_column()
            .width(SizeDimension::Percent(1.0))
            .height(SizeDimension::Percent(1.0)),
        move |_| RectStyle::filled(background, radius),
        children,
    )?;
    *card_rect.borrow_mut() = track_layout(card.layout_node());
    Ok(box_item(card))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::press;
    use layout_core::AvailableSpace;
    use platform_core::{Event, PointerSource};
    use ui_core::{Component, compute_layout, reset_layout_runtime};

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
    /// `on_drag` reports a position *local to the grip*, so a grip that forgot to add its own origin back would
    /// resize the window to about 14×14 the moment it was touched. And the grab offset — the distance from the
    /// pointer to the corner when the drag began — is what stops the corner teleporting to the cursor on the
    /// first event: press the middle of the grip and the window must not change size at all.
    #[test]
    fn the_grip_resizes_by_the_distance_dragged_not_to_the_pointer() {
        use std::cell::RefCell;
        reset_layout_runtime();

        let asked: Rc<RefCell<Vec<(f32, f32)>>> = Rc::new(RefCell::new(Vec::new()));
        let sink = Rc::clone(&asked);
        let style = SurfaceFrameStyle {
            background: Color::TRANSPARENT,
            title_bar: Color::TRANSPARENT,
            title_text: Color::TRANSPARENT,
            close: Color::TRANSPARENT,
            radius: 0.0,
            font_size: 12.0,
        };
        let mut frame = window_frame(
            "Settings",
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

        // The grip sits at the card's bottom-right, inset by the row's padding.
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
    /// A settings-sized float hands `surface_frame` a body sized to fill the window — its page area is a scroll
    /// leaf with a definite height, computed from the surface height less the chrome that existed before there
    /// was a grip. A body that will not shrink below its content pushes the grip row past the bottom edge, and
    /// the affordance builds, lays out, and is never visible. Which is exactly what happened.
    #[test]
    fn the_grip_stays_inside_a_window_whose_body_wants_all_of_it() {
        reset_layout_runtime();

        const SURFACE: (f32, f32) = (920.0, 680.0);
        let style = SurfaceFrameStyle {
            background: Color::TRANSPARENT,
            title_bar: Color::TRANSPARENT,
            title_text: Color::TRANSPARENT,
            close: Color::TRANSPARENT,
            radius: 0.0,
            font_size: 12.0,
        };
        // Taller than the surface, the way an application body is once its own chrome is added on top.
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
        let sink = Rc::clone(&asked);
        let mut frame = window_frame(
            "Settings",
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

        // Pressing the bottom-right corner and dragging is the property the user actually has: a grip laid out past the bottom edge receives nothing, so nothing resizes.
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
        reset_layout_runtime();
        let style = SurfaceFrameStyle {
            background: Color::TRANSPARENT,
            title_bar: Color::TRANSPARENT,
            title_text: Color::TRANSPARENT,
            close: Color::TRANSPARENT,
            radius: 0.0,
            font_size: 12.0,
        };
        // A grip a backend cannot act on must be absent rather than present and inert — an affordance that does nothing is worse than none.
        assert!(window_frame("Clock", style, Rc::new(|| {}), panel(), None).is_ok());
    }
}
