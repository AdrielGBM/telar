//! Titled, closable window chrome with an optional resize grip.
//!
//! It lived in `ui-core` while that crate was the only place a full-surface root could be assembled, and it never belonged there: it is `Text` plus two `StyledContainer`s plus a closure over `track_layout` — a composed widget using nothing the primitive layer has that the catalogue lacks. "Titled closable window chrome" is a catalogue entry, not a layout primitive.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use telar::{
    AlignItems, Color, JustifyContent, LayoutError, LayoutItem, LayoutStyle, Reactive, Rect,
    RectStyle, RwSignal, ShapeStyle, SizeDimension, StyledContainer, Text, TextStyle, box_item,
    track_layout,
};

use crate::shared;

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

#[derive(Clone)]
/// How a window frame is painted: its card, its title strip and its resize grip.
///
/// Colours are read each time the frame paints, so one that follows a theme restyles the frame on a mode switch without rebuilding it. Set them with the builder methods, which take a plain [`Color`], a signal or a [`Reactive::of`] derivation alike.
pub struct SurfaceFrameStyle {
    pub background: Reactive<Color>,
    pub title_bar: Reactive<Color>,
    pub title_text: Reactive<Color>,
    pub close: Reactive<Color>,
    pub radius: f32,
    pub font_size: f32,
    /// The controls beside close. Default is none, which is the frame as it was.
    pub controls: WindowControls,
    /// The inset around `body`. A floating panel wants its content off the edges; an application window whose content owns the whole area below the title strip wants `0.0`, and would otherwise get a border of background colour it never asked for.
    pub body_inset: f32,
    /// Fill behind minimize/maximize under the pointer, and behind close — which is usually the louder of the two, since closing is the one control that cannot be undone.
    ///
    /// Both default to transparent, i.e. no hover feedback, because a panel whose only control is a ✕ in the corner of a translucent surface has nothing to highlight against. A real window title bar sets them.
    pub control_hover: Reactive<Color>,
    pub close_hover: Reactive<Color>,
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
    color: Reactive<Color>,
    card_rect: DeferredRect,
    resize: Rc<dyn Fn(f32, f32)>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let grip = StyledContainer::new(
        LayoutStyle::new().width(GRIP_SIZE).height(GRIP_SIZE),
        move |_| RectStyle::filled(color.get(), 2.0),
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
    let title_color = style.title_text.clone();
    let font_size = style.font_size;
    let title_label = box_item(Text::declaring(
        move || title.clone(),
        LayoutStyle::new(),
        move |inherited| {
            shared::in_family_of(TextStyle::new(font_size, title_color.get()), inherited)
        },
    )?);

    let close_color = style.close.clone();
    let close_glyph_color = close_color.clone();
    let close_label = box_item(Text::declaring(
        || "\u{2715}".to_string(),
        LayoutStyle::new(),
        move |inherited| {
            shared::in_family_of(
                TextStyle::new(font_size, close_glyph_color.get()),
                inherited,
            )
        },
    )?);
    let close_hover = style.close_hover.clone();
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
        .hover_style(move |_| RectStyle::default().with_fill(close_hover.get()))
        .on_press(move || close()),
    );

    // The same shape as the close button beside them, so a frame with three controls reads as one strip rather than a button and two additions.
    let control_hover = style.control_hover.clone();
    let control_button = |glyph: &'static str,
                          command: telar::WindowCommand|
     -> Result<Box<dyn LayoutItem>, LayoutError> {
        let glyph_color = close_color.clone();
        let hover = control_hover.clone();
        let label = box_item(Text::declaring(
            move || glyph.to_string(),
            LayoutStyle::new(),
            move |inherited| {
                shared::in_family_of(TextStyle::new(font_size, glyph_color.get()), inherited)
            },
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
            .hover_style(move |_| RectStyle::default().with_fill(hover.get()))
            .on_press(move || telar::push_window_command(command.clone())),
        ))
    };

    let mut controls: Vec<Box<dyn LayoutItem>> = Vec::new();
    if style.controls.minimize {
        controls.push(control_button("\u{2013}", telar::WindowCommand::Minimize)?);
    }
    if style.controls.maximize {
        controls.push(control_button(
            "\u{25a1}",
            telar::WindowCommand::ToggleMaximize,
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

    let title_bar_color = style.title_bar.clone();
    let drag_moves = style.controls.drag;
    let title_strip = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::SPACE_BETWEEN)
            .width(SizeDimension::Percent(1.0))
            .padding_horizontal(12.0)
            .padding_vertical(8.0),
        move |_| RectStyle::filled(title_bar_color.get(), 0.0),
        vec![label_group, controls],
    )?;
    // The strip is what a user grabs to move the window, so the drag lives here and not on the card: the body is content, and dragging content is a selection everywhere else.
    let title_bar = box_item(if drag_moves {
        title_strip.on_drag(|_, _| telar::push_window_command(telar::WindowCommand::Drag))
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
            vec![resize_grip(style.close.clone(), card_rect.clone(), resize)?],
        )?));
    }

    let background = style.background.clone();
    let radius = style.radius;
    let card = StyledContainer::new(
        LayoutStyle::new()
            .flex_column()
            .width(SizeDimension::Percent(1.0))
            .height(SizeDimension::Percent(1.0)),
        move |_| RectStyle::filled(background.get(), radius),
        children,
    )?;
    *card_rect.borrow_mut() = track_layout(card.layout_node());
    Ok(box_item(card))
}

impl SurfaceFrameStyle {
    pub fn background(mut self, color: impl Into<Reactive<Color>>) -> Self {
        self.background = color.into();
        self
    }

    pub fn title_bar(mut self, color: impl Into<Reactive<Color>>) -> Self {
        self.title_bar = color.into();
        self
    }

    pub fn title_text(mut self, color: impl Into<Reactive<Color>>) -> Self {
        self.title_text = color.into();
        self
    }

    pub fn close(mut self, color: impl Into<Reactive<Color>>) -> Self {
        self.close = color.into();
        self
    }

    pub fn control_hover(mut self, color: impl Into<Reactive<Color>>) -> Self {
        self.control_hover = color.into();
        self
    }

    pub fn close_hover(mut self, color: impl Into<Reactive<Color>>) -> Self {
        self.close_hover = color.into();
        self
    }
}

/// A frame with no colours of its own and no controls — what a caller fills in. Exists so adding a field to this struct does not break every construction of it.
impl Default for SurfaceFrameStyle {
    fn default() -> Self {
        Self {
            background: Color::TRANSPARENT.into(),
            title_bar: Color::TRANSPARENT.into(),
            title_text: Color::TRANSPARENT.into(),
            close: Color::TRANSPARENT.into(),
            radius: 0.0,
            font_size: 14.0,
            controls: WindowControls::default(),
            body_inset: 12.0,
            control_hover: Color::TRANSPARENT.into(),
            close_hover: Color::TRANSPARENT.into(),
        }
    }
}

#[cfg(test)]
#[path = "window_frame_test.rs"]
mod tests;
