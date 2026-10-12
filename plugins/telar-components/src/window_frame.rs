//! Titled, closable window chrome with an optional resize grip.
//!
//! It lived in `ui-core` while that crate was the only place a full-surface root could be assembled, and it never belonged there: it is `Text` plus two `StyledContainer`s plus a closure over `track_layout` — a composed widget using nothing the primitive layer has that the catalogue lacks. "Titled closable window chrome" is a catalogue entry, not a layout primitive.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use telar::{
    Accessible, AlignItems, BorderRadius, Color, JustifyContent, LayoutError, LayoutItem,
    LayoutStyle, Reactive, Rect, RectStyle, RwSignal, ShapeStyle, SizeDimension, StyledContainer,
    SurfaceAmendment, Text, TextStyle, amend_surface, box_item, focus::Role, signal, track_layout,
};

use crate::shared;
use crate::strings;

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

/// How one window frame is painted: its card, its title strip, its controls and its resize grip.
///
/// The backend that opens a surface supplies it, since a layer-shell panel over a translucent desktop and an application window want different chrome under one theme. Every field left `None` follows the theme role named beside it, and colours are read each time the frame paints, so the frame restyles on a theme or mode switch without being rebuilt. The builder methods take a plain [`Color`], a signal or a [`Reactive::of`] derivation alike.
#[derive(Clone, Default)]
pub struct SurfaceFrameStyle {
    /// The card behind the strip and the body. `None` is the theme's `surface`.
    pub background: Option<Reactive<Color>>,
    /// `None` is the theme's `surface_alt`.
    pub title_bar: Option<Reactive<Color>>,
    /// The title's ink. `None` keeps the ink the frame inherits.
    pub title_text: Option<Reactive<Color>>,
    /// The controls' glyphs and the resize grip. `None` keeps the inherited ink for the glyphs and draws the grip in the theme's `muted`.
    pub close: Option<Reactive<Color>>,
    /// The card's corner radius. `None` is square, the shape a window system gives a window; a floating panel passes a role such as [`metrics::radius_dialog`](crate::metrics::radius_dialog).
    pub radius: Option<f32>,
    /// The controls beside close. Default is none.
    pub controls: WindowControls,
    /// The inset around `body`, in logical px. `None` is [`metrics::gap_lg`](crate::metrics::gap_lg); an application window whose content owns the whole area below the title strip passes `Some(0.0)`, and would otherwise get a border of background colour it never asked for.
    pub body_inset: Option<f32>,
    /// Fill behind minimize/maximize under the pointer, and behind close — often the louder of the two, since closing is the one control that cannot be undone. `None` is the theme's hover wash for both.
    pub control_hover: Option<Reactive<Color>>,
    pub close_hover: Option<Reactive<Color>>,
    /// The glyph's colour while the pointer is over the control, for a hover fill the resting glyph colour cannot be read against. `None` keeps the glyph as it is.
    pub control_hover_glyph: Option<Reactive<Color>>,
    pub close_hover_glyph: Option<Reactive<Color>>,
    /// Amends the paint of the card — the frame's **principal surface**. See [`telar::SurfaceStyle`] for when a theme token is the right instrument instead.
    pub card: Option<SurfaceAmendment>,
}

/// The smallest a frame will ask to become. A window dragged to nothing is a window the user cannot get hold of again — its own grip goes with it.
pub const MIN_FRAME_SIZE: (f32, f32) = (180.0, 120.0);

/// `color` when the frame was given one, else what `fallback` reads from the theme.
fn or_theme(color: &Option<Reactive<Color>>, fallback: impl FnOnce() -> Color) -> Color {
    color.as_ref().map_or_else(fallback, Reactive::get)
}

/// The text of the title and of the controls' glyphs: the frame's inherited text, in `color` when one was given.
fn chrome_text(inherited: TextStyle, color: Option<Color>) -> TextStyle {
    let text = shared::control_text(inherited, 1.0);
    match color {
        Some(color) => text.with_color(color),
        None => text,
    }
}

fn grip_box() -> LayoutStyle {
    LayoutStyle::new()
        .width(shared::icon_size())
        .height(shared::icon_size())
}

/// The rect a grip measures the frame against. It is a cell rather than a signal because the grip has to exist before the row that holds it, and that row before the card that holds *both* — so the one rect the grip needs is the one thing it cannot be handed at construction. Filled in as soon as the card exists.
type DeferredRect = Rc<RefCell<Option<RwSignal<Rect>>>>;

/// A resize grip for the bottom-right corner of a frame, reporting the size the *surface* should become.
///
/// The arithmetic is the whole of it. `on_drag` reports where the pointer is **inside the grip**, so the grip's own laid-out origin has to be added back to reach surface space — and then the grab offset, the distance from the pointer to the corner when the drag began, has to come off it, or the corner jumps to the cursor the instant it is touched. The offset is latched once per drag rather than recomputed, because the card it was measured against is resizing underneath the gesture.
fn resize_grip(
    color: Option<Reactive<Color>>,
    card_rect: DeferredRect,
    resize: Rc<dyn Fn(f32, f32)>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let grip = StyledContainer::new(
        grip_box(),
        move |_| RectStyle::filled(or_theme(&color, shared::muted), shared::radius_item()),
        vec![],
    )?
    .styled_by(grip_box);
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

/// What a window-management button is painted in: its mark at rest, and its fill and mark under the pointer.
struct ControlPaint {
    glyph: Option<Reactive<Color>>,
    hover: Option<Reactive<Color>>,
    hover_glyph: Option<Reactive<Color>>,
}

/// One of the window-management buttons in the strip: a mark a reader hears by `name`.
fn control_button(
    shape: shared::Mark,
    name: &'static str,
    paint: ControlPaint,
    press: Rc<dyn Fn()>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let ControlPaint {
        glyph: rest,
        hover,
        hover_glyph,
    } = paint;
    let hovered = signal(false);
    let label = shared::mark(
        move || shape,
        move |inherited| {
            let color = match &hover_glyph {
                Some(hover_glyph) if hovered.get() => Some(hover_glyph.get()),
                _ => rest.as_ref().map(Reactive::get),
            };
            chrome_text(inherited.clone(), color).color
        },
    )?;
    let shell = || {
        LayoutStyle::new()
            .flex_column()
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER)
            .padding_horizontal(shared::gap_md())
            .padding_vertical(shared::gap_xs())
            .min_width(shared::MIN_TARGET)
            .min_height(shared::MIN_TARGET)
    };
    Ok(box_item(
        StyledContainer::new(shell(), |_| RectStyle::default(), vec![label])?
            .styled_by(shell)
            .hover_style(move |_| {
                RectStyle::default().with_fill(or_theme(&hover, shared::hover_wash))
            })
            .on_hover(move |over| hovered.set(over))
            .control(Role::Button)
            .a11y_label(move || strings::text(name))
            .on_press(move || press()),
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
    let title_label = box_item(Text::declaring(
        move || title.clone(),
        LayoutStyle::new(),
        move |inherited| chrome_text(inherited, title_color.as_ref().map(Reactive::get)),
    )?);

    let window_command = |command: telar::WindowCommand| -> Rc<dyn Fn()> {
        Rc::new(move || telar::push_window_command(command.clone()))
    };
    let control_paints = || ControlPaint {
        glyph: style.close.clone(),
        hover: style.control_hover.clone(),
        hover_glyph: style.control_hover_glyph.clone(),
    };

    let mut controls: Vec<Box<dyn LayoutItem>> = Vec::new();
    if style.controls.minimize {
        controls.push(control_button(
            shared::Mark::Minimize,
            strings::MINIMIZE,
            control_paints(),
            window_command(telar::WindowCommand::Minimize),
        )?);
    }
    if style.controls.maximize {
        controls.push(control_button(
            shared::Mark::Maximize,
            strings::MAXIMIZE,
            control_paints(),
            window_command(telar::WindowCommand::ToggleMaximize),
        )?);
    }
    controls.push(control_button(
        shared::Mark::Close,
        strings::CLOSE,
        ControlPaint {
            glyph: style.close.clone(),
            hover: style.close_hover.clone(),
            hover_glyph: style.close_hover_glyph.clone(),
        },
        close,
    )?);
    let controls = box_item(StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER),
        |_| RectStyle::default(),
        controls,
    )?);

    // One group, so `SPACE_BETWEEN` pushes the controls to the far edge rather than spreading three things across the strip.
    let label_box = || {
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .gap(shared::gap_md())
    };
    let label_group = box_item(
        StyledContainer::new(
            label_box(),
            |_| RectStyle::default(),
            match leading {
                Some(leading) => vec![leading, title_label],
                None => vec![title_label],
            },
        )?
        .styled_by(label_box),
    );

    let title_bar_color = style.title_bar.clone();
    let drag_moves = style.controls.drag;
    let strip_box = || {
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::SPACE_BETWEEN)
            .width(SizeDimension::Percent(1.0))
            .padding_horizontal(shared::gap_lg())
            .padding_vertical(shared::gap_md())
    };
    let title_strip = StyledContainer::new(
        strip_box(),
        move |_| RectStyle::default().with_fill(or_theme(&title_bar_color, shared::surface_alt)),
        vec![label_group, controls],
    )?
    .styled_by(strip_box);
    // The strip is what a user grabs to move the window, so the drag lives here and not on the card: the body is content, and dragging content is a selection everywhere else.
    let title_bar = box_item(if drag_moves {
        title_strip.on_drag(|_, _| telar::push_window_command(telar::WindowCommand::Drag))
    } else {
        title_strip
    });

    // A flex item may not shrink below its content unless told to, and an application body sized to fill the window otherwise pushes the grip row off the bottom of the surface.
    let body_inset = style.body_inset;
    let body_box = move || {
        LayoutStyle::new()
            .flex_column()
            .flex_grow(1.0)
            .min_height(0.0)
            .width(SizeDimension::Percent(1.0))
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER)
            .padding_all(body_inset.unwrap_or_else(shared::gap_lg))
    };
    let body_area = box_item(
        StyledContainer::new(body_box(), |_| RectStyle::default(), vec![body])?.styled_by(body_box),
    );

    let card_rect: DeferredRect = Rc::new(RefCell::new(None));
    let mut children = vec![title_bar, body_area];
    if let Some(resize) = resize {
        let grip_row = || {
            LayoutStyle::new()
                .flex_row()
                .width(SizeDimension::Percent(1.0))
                .flex_shrink(0.0)
                .justify_content(JustifyContent::END)
                .padding_horizontal(shared::gap_sm())
                .padding_bottom(shared::gap_sm())
        };
        children.push(box_item(
            StyledContainer::new(
                grip_row(),
                |_| RectStyle::default(),
                vec![resize_grip(style.close.clone(), card_rect.clone(), resize)?],
            )?
            .styled_by(grip_row),
        ));
    }

    let SurfaceFrameStyle {
        background,
        radius,
        card: amendment,
        ..
    } = style;
    let card = StyledContainer::new(
        LayoutStyle::new()
            .flex_column()
            .width(SizeDimension::Percent(1.0))
            .height(SizeDimension::Percent(1.0)),
        move |_| {
            let card = RectStyle::default()
                .with_fill(or_theme(&background, shared::surface))
                .with_radius(BorderRadius::all(radius.unwrap_or_default()));
            amend_surface(card, &amendment)
        },
        children,
    )?;
    *card_rect.borrow_mut() = track_layout(card.layout_node());
    Ok(box_item(card))
}

impl SurfaceFrameStyle {
    pub fn background(mut self, color: impl Into<Reactive<Color>>) -> Self {
        self.background = Some(color.into());
        self
    }

    pub fn title_bar(mut self, color: impl Into<Reactive<Color>>) -> Self {
        self.title_bar = Some(color.into());
        self
    }

    pub fn title_text(mut self, color: impl Into<Reactive<Color>>) -> Self {
        self.title_text = Some(color.into());
        self
    }

    pub fn close(mut self, color: impl Into<Reactive<Color>>) -> Self {
        self.close = Some(color.into());
        self
    }

    pub fn control_hover(mut self, color: impl Into<Reactive<Color>>) -> Self {
        self.control_hover = Some(color.into());
        self
    }

    pub fn close_hover(mut self, color: impl Into<Reactive<Color>>) -> Self {
        self.close_hover = Some(color.into());
        self
    }

    pub fn control_hover_glyph(mut self, color: impl Into<Reactive<Color>>) -> Self {
        self.control_hover_glyph = Some(color.into());
        self
    }

    pub fn close_hover_glyph(mut self, color: impl Into<Reactive<Color>>) -> Self {
        self.close_hover_glyph = Some(color.into());
        self
    }

    /// Amends the card's paint after the frame has worked it out.
    pub fn card(mut self, amendment: SurfaceAmendment) -> Self {
        self.card = Some(amendment);
        self
    }
}

#[cfg(test)]
#[path = "window_frame_test.rs"]
mod tests;
