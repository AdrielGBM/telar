//! Titled, closable window chrome with an optional resize grip.
//!
//! It lived in `ui-core` while that crate was the only place a full-surface root could be assembled, and it never belonged there: it is `Text` plus two `StyledContainer`s plus a closure over `track_layout` — a composed widget using nothing the primitive layer has that the catalogue lacks. "Titled closable window chrome" is a catalogue entry, not a layout primitive.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use geometry_core::Rect;
use layout_core::{AlignItems, JustifyContent, LayoutError, LayoutStyle, SizeDimension};
use reactive_core::RwSignal;
use renderer_core::{Color, RectStyle, ShapeStyle, TextStyle};
use ui_core::{LayoutItem, StyledContainer, Text, box_item, track_layout};

/// Which window-management controls a frame draws, beside the close button it always has.
///
/// Off by default: a layer-shell panel has no top-level window to minimize, and drawing a control that does nothing is worse than not drawing it. A windowed backend turns on what its platform can honour.
#[derive(Debug, Clone, Copy, Default)]
pub struct WindowControls {
    /// Dragging the title strip asks the platform for an interactive move.
    pub drag: bool,
    pub minimize: bool,
    pub maximize: bool,
}

#[derive(Debug, Clone, Copy)]
/// How a window frame is painted: its card, its title strip and its resize grip.
pub struct SurfaceFrameStyle {
    pub background: Color,
    pub title_bar: Color,
    pub title_text: Color,
    pub close: Color,
    pub radius: f32,
    pub font_size: f32,
    /// The controls beside close. Default is none, which is the frame as it was.
    pub controls: WindowControls,
    /// The inset around `body`. A floating panel wants its content off the edges; an application window whose content owns the whole area below the title strip wants `0.0`, and would otherwise get a border of background colour it never asked for.
    pub body_inset: f32,
    /// Fill behind minimize/maximize under the pointer, and behind close — which is usually the louder of the two, since closing is the one control that cannot be undone.
    ///
    /// Both default to transparent, i.e. no hover feedback, because a panel whose only control is a ✕ in the corner of a translucent surface has nothing to highlight against. A real window title bar sets them.
    pub control_hover: Color,
    pub close_hover: Color,
}

/// The smallest a frame will ask to become. A window dragged to nothing is a window the user cannot get hold of again — its own grip goes with it.
pub const MIN_FRAME_SIZE: (f32, f32) = (180.0, 120.0);

/// The corner grip's side, in logical pixels. Big enough to hit without aiming, small enough not to read as content.
const GRIP_SIZE: f32 = 14.0;

/// The rect a grip measures the frame against. It is a cell rather than a signal because the grip has to exist before the row that holds it, and that row before the card that holds *both* — so the one rect the grip needs is the one thing it cannot be handed at construction. Filled in as soon as the card exists.
type DeferredRect = Rc<RefCell<Option<RwSignal<Rect>>>>;

/// A resize grip for the bottom-right corner of a frame, reporting the size the *surface* should become.
///
/// The arithmetic is the whole of it. `on_drag` reports where the pointer is **inside the grip**, so the grip's own laid-out origin has to be added back to reach surface space — and then the grab offset, the distance from the pointer to the corner when the drag began, has to come off it, or the corner jumps to the cursor the instant it is touched. The offset is latched once per drag rather than recomputed, because the card it was measured against is resizing underneath the gesture.
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
    let release = grab.clone();
    Ok(box_item(
        grip.on_drag(move |local_x, local_y| {
            let (Some(grip_rect), Some(card_rect)) = (&grip_rect, *card_rect.borrow()) else {
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
/// `leading` is drawn before the title — an application icon, a back arrow, a status dot. `None` for a frame that is only a title, which is every panel.
///
/// `resize` opts the frame into a corner grip: it is handed the size the surface should take, in logical pixels, on every move of that grip. A backend that can renegotiate a surface's size wires it up; one that cannot passes `None` and the grip is not drawn, rather than drawn and inert.
pub fn window_frame(
    title: impl Into<String>,
    leading: Option<Box<dyn LayoutItem>>,
    style: SurfaceFrameStyle,
    close: std::rc::Rc<dyn Fn()>,
    body: Box<dyn LayoutItem>,
    resize: Option<Rc<dyn Fn(f32, f32)>>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let title = title.into();
    let title_color = style.title_text;
    let font_size = style.font_size;
    let title_label = box_item(Text::new(
        move || title.clone(),
        LayoutStyle::new(),
        move || TextStyle::new(font_size, title_color),
    )?);

    let close_color = style.close;
    let close_label = box_item(Text::new(
        || "\u{2715}".to_string(),
        LayoutStyle::new(),
        move || TextStyle::new(font_size, close_color),
    )?);
    let close_hover = style.close_hover;
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
        .hover_style(move |_| RectStyle::default().with_fill(close_hover))
        .on_press(move || close()),
    );

    // The same shape as the close button beside them, so a frame with three controls reads as one strip rather than a button and two additions.
    let control_hover = style.control_hover;
    let control_button = |glyph: &'static str,
                          command: platform_core::WindowCommand|
     -> Result<Box<dyn LayoutItem>, LayoutError> {
        let label = box_item(Text::new(
            move || glyph.to_string(),
            LayoutStyle::new(),
            move || TextStyle::new(font_size, close_color),
        )?);
        Ok(box_item(
            StyledContainer::new(
                LayoutStyle::new()
                    .align_items(AlignItems::CENTER)
                    .justify_content(JustifyContent::CENTER)
                    .padding_horizontal(8.0)
                    .padding_vertical(2.0),
                |_| RectStyle::default(),
                vec![label],
            )?
            .hover_style(move |_| RectStyle::default().with_fill(control_hover))
            .on_press(move || platform_core::push_window_command(command.clone())),
        ))
    };

    let mut controls: Vec<Box<dyn LayoutItem>> = Vec::new();
    if style.controls.minimize {
        controls.push(control_button(
            "\u{2013}",
            platform_core::WindowCommand::Minimize,
        )?);
    }
    if style.controls.maximize {
        controls.push(control_button(
            "\u{25a1}",
            platform_core::WindowCommand::ToggleMaximize,
        )?);
    }
    controls.push(close_button);
    let controls = box_item(StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER),
        |_| RectStyle::default(),
        controls,
    )?);

    // One group, so `SPACE_BETWEEN` pushes the controls to the far edge rather than spreading three things across the strip.
    let label_group = box_item(StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .gap(8.0),
        |_| RectStyle::default(),
        match leading {
            Some(leading) => vec![leading, title_label],
            None => vec![title_label],
        },
    )?);

    let title_bar_color = style.title_bar;
    let drag_moves = style.controls.drag;
    let title_strip = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::SPACE_BETWEEN)
            .width(SizeDimension::Percent(1.0))
            .padding_horizontal(12.0)
            .padding_vertical(8.0),
        move |_| RectStyle::filled(title_bar_color, 0.0),
        vec![label_group, controls],
    )?;
    // The strip is what a user grabs to move the window, so the drag lives here and not on the card: the body is content, and dragging content is a selection everywhere else.
    let title_bar = box_item(if drag_moves {
        title_strip
            .on_drag(|_, _| platform_core::push_window_command(platform_core::WindowCommand::Drag))
    } else {
        title_strip
    });

    // A flex item may not shrink below its content unless told to, and an application body sized to fill the window otherwise pushes the grip row off the bottom of the surface.
    let body_area = box_item(StyledContainer::new(
        LayoutStyle::new()
            .flex_column()
            .flex_grow(1.0)
            .min_height(0.0)
            .width(SizeDimension::Percent(1.0))
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER)
            .padding_all(style.body_inset),
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
            vec![resize_grip(style.close, card_rect.clone(), resize)?],
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

/// A frame with no colours of its own and no controls — what a caller fills in. Exists so adding a field to this struct does not break every construction of it.
impl Default for SurfaceFrameStyle {
    fn default() -> Self {
        Self {
            background: Color::TRANSPARENT,
            title_bar: Color::TRANSPARENT,
            title_text: Color::TRANSPARENT,
            close: Color::TRANSPARENT,
            radius: 0.0,
            font_size: 14.0,
            controls: WindowControls::default(),
            body_inset: 12.0,
            control_hover: Color::TRANSPARENT,
            close_hover: Color::TRANSPARENT,
        }
    }
}

#[cfg(test)]
#[path = "window_frame_test.rs"]
mod tests;
