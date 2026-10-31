//! Titled, closable window chrome with an optional resize grip.
//!
//! It lived in `ui-core` while that crate was the only place a full-surface root could be assembled, and it never belonged there: it is `Text` plus two `StyledContainer`s plus a closure over `track_layout` — a composed widget using nothing the primitive layer has that the catalogue lacks. "Titled closable window chrome" is a catalogue entry, not a layout primitive.

use std::cell::RefCell;
use std::rc::Rc;

use telar::{
    AlignItems, BorderRadius, Children, Color, Container, JustifyContent, LayoutError, LayoutItem,
    LayoutStyle, Props, Reactive, Rect, RectStyle, RwSignal, ShapeStyle, Size, SizeDimension,
    StyledContainer, SurfaceAmendment, Text, TextStyle, amend_surface, box_item, focus::Role,
    track_layout,
};

use crate::resize::Latch;
use crate::shared;
use crate::title_bar::{TitleBarOptions, WindowControlsProps, title_bar, window_controls};

/// Which window-management controls a frame draws, beside the close button it always has.
///
/// Off by default: a layer-shell panel has no top-level window to minimize, and drawing a control that does nothing is worse than not drawing it. A windowed backend turns on what its platform can honour.
#[derive(Debug, Clone, Copy, Default)]
pub struct WindowControls {
    /// The title strip is the window's title bar, as [`title_bar`](crate::title_bar) makes one: dragging it asks the platform for an interactive move, and double-clicking it maximizes or restores the window where `maximize` is on too.
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
/// The surface grows by the distance the pointer travelled from where it pressed the grip, measured in surface space and against the card's size when the drag took hold, so the corner neither jumps to the cursor nor drifts as the card resizes under the gesture.
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
    let latch = Rc::new(Latch::default());
    let held = latch.clone();
    Ok(box_item(
        grip.on_drag(move |local_x, local_y| {
            let (Some(grip_rect), Some(card_rect)) = (&grip_rect, *card_rect.borrow()) else {
                return;
            };
            let (grab, pointer) = latch.hold(grip_rect.get(), (local_x, local_y), || {
                let card = card_rect.get();
                Size::new(card.width, card.height)
            });
            let (dx, dy) = grab.travel(pointer);
            resize(
                (grab.from.width + dx).max(MIN_FRAME_SIZE.0),
                (grab.from.height + dy).max(MIN_FRAME_SIZE.1),
            );
        })
        .on_drag_end(move |_, _| {
            held.release();
        }),
    ))
}

/// A titled, closable window frame around its [`body`](Self::body).
#[derive(Props)]
pub struct WindowFrameProps {
    /// What the title strip says.
    #[props(into, default)]
    pub title: Reactive<String>,
    /// Drawn before the title: an application icon, a back arrow, a status dot. Nothing (the default) for a frame that is only a title, which is every panel.
    #[props(default)]
    pub leading: Children,
    /// How the frame is painted, and which window controls it draws beside close.
    #[props(default)]
    pub style: SurfaceFrameStyle,
    /// What the close button does.
    pub close: Rc<dyn Fn()>,
    /// What the frame holds, under the title strip.
    #[props(default)]
    pub body: Children,
    /// Opts the frame into a corner grip, handed the size the surface should take, in logical pixels, on every move of that grip. A backend that can renegotiate a surface's size wires it up; one that cannot leaves it `None` (the default) and the grip is not drawn, rather than drawn and inert.
    #[props(some, default)]
    pub resize: Option<Rc<dyn Fn(f32, f32)>>,
}

/// A titled, closable window frame around its body.
pub fn window_frame(props: WindowFrameProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let WindowFrameProps {
        title,
        leading,
        style,
        close,
        body,
        resize,
    } = props;
    let title_color = style.title_text.clone();
    let title_label = box_item(Text::declaring(
        move || title.get(),
        LayoutStyle::new(),
        move |inherited| chrome_text(inherited, title_color.as_ref().map(Reactive::get)),
    )?);

    let or_unset = |color: &Option<Reactive<Color>>| {
        color.clone().unwrap_or_else(|| Color::TRANSPARENT.into())
    };
    let controls = window_controls(WindowControlsProps {
        minimize: style.controls.minimize,
        maximize: style.controls.maximize,
        on_close: close,
        glyph: or_unset(&style.close),
        hover: or_unset(&style.control_hover),
        hover_glyph: or_unset(&style.control_hover_glyph),
        close_hover: or_unset(&style.close_hover),
        close_hover_glyph: or_unset(&style.close_hover_glyph),
        min_size: None,
        style: None,
    })?;

    // One group, so `SPACE_BETWEEN` pushes the controls to the far edge rather than spreading three things across the strip.
    let label_box = || {
        LayoutStyle::row()
            .align_items(AlignItems::CENTER)
            .gap(shared::gap_md())
    };
    let mut labels = leading.build()?;
    labels.push(title_label);
    let label_group = box_item(Container::new(label_box(), labels)?.styled_by(label_box));

    let title_bar_color = style.title_bar.clone();
    let strip_box = || {
        LayoutStyle::row()
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
    .styled_by(strip_box)
    .role(Role::Group);
    // The strip is what a user grabs to move the window, so the drag lives here and not on the card: the body is content, and dragging content is a selection everywhere else.
    let strip = box_item(title_bar(
        title_strip,
        TitleBarOptions {
            drag: style.controls.drag,
            maximize: style.controls.drag && style.controls.maximize,
            replace_decorations: false,
        },
    ));

    // A flex item may not shrink below its content unless told to, and an application body sized to fill the window otherwise pushes the grip row off the bottom of the surface.
    let body_inset = style.body_inset;
    let body_box = move || {
        LayoutStyle::column()
            .flex_grow(1.0)
            .min_height(0.0)
            .width(SizeDimension::Percent(1.0))
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER)
            .padding_all(body_inset.unwrap_or_else(shared::gap_lg))
    };
    let body_area = box_item(Container::new(body_box(), body.build()?)?.styled_by(body_box));

    let card_rect: DeferredRect = Rc::new(RefCell::new(None));
    let mut children = vec![strip, body_area];
    if let Some(resize) = resize {
        let grip_row = || {
            LayoutStyle::row()
                .width(SizeDimension::Percent(1.0))
                .flex_shrink(0.0)
                .justify_content(JustifyContent::END)
                .padding_horizontal(shared::gap_sm())
                .padding_bottom(shared::gap_sm())
        };
        children.push(box_item(
            Container::new(
                grip_row(),
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
        LayoutStyle::column()
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
