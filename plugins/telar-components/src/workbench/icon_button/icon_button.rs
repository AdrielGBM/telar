//! [`icon_button`]: a square button drawn from an icon, named by a tooltip and by a screen reader.

use std::rc::Rc;

use telar::{
    Accessible, AlignItems, AuthoredText, BorderRadius, Chord, Color, Declared, Handler, IconName,
    JustifyContent, KeySequence, LayoutError, LayoutItem, LayoutStyle, Props, Reactive, RectStyle,
    ShapeStyle, StyledContainer, SurfaceAmendment, amend_surface, box_item, focus::Role,
};

use crate::shared;
use crate::workbench::toolbar::toolbar::ToolbarContext;
use telar::ControlSize;

/// A button that is only a picture, so its name is not optional: `label` is what the tooltip says and what a screen reader announces.
#[derive(Props)]
pub struct IconButtonProps {
    /// The icon, `set:name`: `icon:"lucide:search"` in `.rsx`, baked where the package bakes icons, or `telar::icon!("lucide:search")` from Rust. Under the `icons` feature `telar-icons` draws it; without it, a Unicode glyph read from its name alone stands in (`x` is ✕, `search` ⚲, `external-link` ↗, the chevrons ‹ › ˄ ˅), and a dot for a name with none.
    #[props(into)]
    pub icon: IconName,
    /// The button's name.
    #[props(into)]
    pub label: Reactive<String>,
    /// The key binding that does the same thing, written portably as [`KeySequence`] reads it (`"Mod+K"`) and shown in the tooltip as the platform writes it. `None` (the default) names none.
    #[props(some, into, default)]
    pub shortcut: Option<KeySequence>,
    /// How large the button is drawn: its square and the icon in it, as [`ControlSize`] works them out. A small one is the smallest pointer target, so it fits a tag's `action` without making the tag taller.
    #[props(default)]
    pub size: ControlSize,
    /// Makes the button a toggle that reads as pressed while this is true. Unset, it is a plain button and says nothing about being pressed.
    #[props(some, into, default)]
    pub pressed: Option<Reactive<bool>>,
    #[props(into, default)]
    pub disabled: Reactive<bool>,
    #[props(into, default = Handler::from(|| {}))]
    pub on_press: Handler,
    /// Amends the paint of the button's square — its **principal surface** — at rest, hovered, pressed and disabled. See [`telar::SurfaceStyle`] for when a theme token or [`IconButtonTokens`] is the right instrument instead.
    #[props(some, default)]
    pub style: Option<SurfaceAmendment>,
}

/// An icon button's own tokens, which an application's theme supplies through a `#[theme(extension)]` field.
///
/// Every field left `None` follows the theme role named beside it, read when the button paints, so a theme that moves the role still moves every icon button it has not given a value of its own.
#[derive(Clone, Debug, Default, PartialEq, telar::ComponentTokens)]
pub struct IconButtonTokens {
    /// Corner radius. `None` is the theme's `radius_control`.
    #[token(or = shared::radius_control)]
    pub radius: Option<f32>,
}

fn shell(size: ControlSize) -> LayoutStyle {
    LayoutStyle::row()
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER)
        .padding_all(shared::icon_pad(size))
        .min_width(shared::control_height(size))
        .min_height(shared::control_height(size))
}

/// Which of its paints the square is showing.
#[derive(Clone, Copy)]
enum State {
    Rest,
    Hovered,
    Pressed,
}

/// The square's paint before the caller's amendment: the theme's washes for a pointer resting on it, for a toggle held on and for a press.
fn paint(state: State, toggled: bool) -> RectStyle {
    let rect =
        RectStyle::default().with_radius(BorderRadius::all(IconButtonTokens::current().radius()));
    let wash = match (state, toggled) {
        (State::Rest, false) => None,
        (State::Hovered, false) => Some(shared::hover_wash()),
        (State::Rest, true) => Some(shared::held_wash()),
        (State::Hovered, true) | (State::Pressed, _) => Some(shared::pressed_wash()),
    };
    match wash {
        Some(wash) => rect.with_fill(wash),
        None => rect,
    }
}

/// A square button drawn from an icon, with a tooltip and an optional pressed state.
///
/// Inside a [`toolbar`](crate::toolbar) it is an item of that toolbar and not a tab stop of its own.
pub fn icon_button(props: IconButtonProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let IconButtonProps {
        icon,
        label,
        size,
        shortcut,
        pressed,
        disabled,
        on_press,
        style,
    } = props;
    let toolbar = telar::use_context::<ToolbarContext>();

    let index = toolbar.as_ref().map(|toolbar| {
        let disabled = disabled.clone();
        toolbar.claim(move || !disabled.get(), on_press.clone())
    });
    let square = Square {
        icon,
        label: label.clone(),
        size,
        shortcut: shortcut.as_ref().and_then(KeySequence::chord).cloned(),
        pressed,
        disabled,
        activate: on_press,
        style,
        toolbar,
        index,
    };

    shared::hinted(move || square.build(), size, label, shortcut)
}

/// Everything the square under an icon button's tooltip is built from, so the tooltip can build it wherever and however often it places it.
#[derive(Clone)]
struct Square {
    icon: IconName,
    label: Reactive<String>,
    size: ControlSize,
    shortcut: Option<Chord>,
    pressed: Option<Reactive<bool>>,
    disabled: Reactive<bool>,
    activate: Handler,
    style: Option<SurfaceAmendment>,
    toolbar: Option<ToolbarContext>,
    index: Option<usize>,
}

impl Square {
    fn build(&self) -> Result<Box<dyn LayoutItem>, LayoutError> {
        let Square {
            icon,
            label,
            size,
            shortcut,
            pressed,
            disabled,
            activate,
            style,
            toolbar,
            index,
        } = self.clone();
        let is_pressed = {
            let pressed = pressed.clone();
            move || pressed.as_ref().is_some_and(|pressed| pressed.get())
        };
        let is_disabled = {
            let disabled = disabled.clone();
            move || disabled.get()
        };
        let cursor = {
            let (toolbar, disabled) = (toolbar.clone(), disabled.clone());
            move || {
                let at_cursor = match (&toolbar, index) {
                    (Some(toolbar), Some(index)) => toolbar.shows_cursor_at(index),
                    _ => false,
                };
                (at_cursor && !disabled.get()).then(shared::focus_ring)
            }
        };

        let picture = picture(icon, size, ink(disabled))?;
        let surface = {
            let is_pressed = is_pressed.clone();
            Rc::new(move |state: State| amend_surface(paint(state, is_pressed()), &style))
        };
        let (rest, hover, pressing, idle) =
            (surface.clone(), surface.clone(), surface.clone(), surface);
        let press = {
            let toolbar = toolbar.clone();
            move || {
                if let (Some(toolbar), Some(index)) = (&toolbar, index) {
                    toolbar.select(index);
                }
                activate.run();
            }
        };
        let button = StyledContainer::new(shell(size), move |_| rest(State::Rest), vec![picture])?
            .styled_by(move || shell(size))
            .hover_style(move |_| hover(State::Hovered))
            .active_style(move |_| pressing(State::Pressed))
            .keyboard_cursor(cursor)
            .disabled(is_disabled)
            .disabled_style(move |_| idle(State::Rest))
            .on_press(press)
            .a11y_label(move || label.get());

        let button = match (&toolbar, index) {
            (Some(toolbar), Some(index)) => {
                let toolbar = toolbar.clone();
                button.presented(Role::Button, move || toolbar.reach(index))
            }
            _ => button.control(Role::Button),
        };
        let button = match shortcut {
            Some(chord) => button.keyboard_shortcut(move || Some(chord.clone())),
            None => button,
        };
        let button = if pressed.is_some() {
            button.toggled(is_pressed)
        } else {
            button
        };
        if let (Some(toolbar), Some(index)) = (&toolbar, index) {
            toolbar.present(index, button.focus_id());
        }
        Ok(box_item(button))
    }
}

fn ink(disabled: Reactive<bool>) -> impl Fn() -> Option<Color> + 'static {
    move || disabled.get().then(shared::muted)
}

/// `icon` at the size a control of `size` draws one, inked by `ink`: in the colour it reads, or in the text around it while it reads `None`.
pub(crate) fn picture(
    icon: IconName,
    size: ControlSize,
    ink: impl Fn() -> Option<Color> + 'static,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    Ok(drawn(icon, size)?.authored_text(move || {
        ink().map_or_else(Declared::default, |ink| Declared::default().with_color(ink))
    }))
}

#[cfg(feature = "icons")]
fn drawn(icon: IconName, size: ControlSize) -> Result<Box<dyn LayoutItem>, LayoutError> {
    use telar_icons::{IconProps, icon as draw};

    draw(
        IconProps::props()
            .name(icon)
            .size(shared::icon_size(size))
            .build(),
    )
}

#[cfg(not(feature = "icons"))]
fn drawn(icon: IconName, size: ControlSize) -> Result<Box<dyn LayoutItem>, LayoutError> {
    use telar::{Container, Text, TextWrap};

    let side = shared::icon_size(size);
    let glyph = Text::declaring(
        move || glyph_for(&icon.id()).to_string(),
        LayoutStyle::new(),
        move |t| shared::control_text(size, t, 1.0).with_text_wrap(TextWrap::NoWrap),
    )?;
    let frame = Container::new(
        LayoutStyle::row()
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER)
            .width(side)
            .height(side)
            .flex_shrink(0.0),
        telar::children![glyph],
    )?
    .a11y_hidden();
    Ok(box_item(frame))
}

/// The Unicode stand-in for an icon id, read by its name alone so any set's `search` draws the same glyph. A name with no stand-in draws a dot rather than nothing, so the button still has a body.
#[cfg(any(test, not(feature = "icons")))]
pub(crate) fn glyph_for(icon: &str) -> &'static str {
    let name = icon.rsplit(':').next().unwrap_or(icon);
    match name {
        "search" | "magnifying-glass" | "zoom" => "⚲",
        "close" | "x" | "cross" => "✕",
        "plus" | "add" => "+",
        "minus" | "remove" => "−",
        "check" | "done" => "✓",
        "settings" | "cog" | "gear" => "⚙",
        "menu" => "☰",
        "ellipsis" | "more-horizontal" | "dots-horizontal" => "…",
        "ellipsis-vertical" | "more-vertical" | "dots-vertical" => "⋮",
        "play" => "▶",
        "pause" => "‖",
        "stop" | "square" => "■",
        "circle" => "●",
        "refresh" | "rotate-cw" | "reload" => "↻",
        "undo" | "rotate-ccw" => "↺",
        "chevron-left" => "‹",
        "chevron-right" => "›",
        "chevron-up" => "˄",
        "chevron-down" => "˅",
        "arrow-left" => "←",
        "arrow-right" => "→",
        "arrow-up" => "↑",
        "arrow-down" => "↓",
        "external-link" | "square-arrow-out-up-right" | "arrow-up-right" | "open-in-new" => "↗",
        "link" => "⚯",
        "home" | "house" => "⌂",
        "pencil" | "edit" => "✎",
        "alert" | "warning" | "triangle-alert" => "⚠",
        "panel-left" | "sidebar" => "◧",
        "star" => "★",
        "info" => "ℹ",
        "copy" => "❐",
        "grid" | "layout-grid" | "grid-3x3" => "▦",
        "sun" => "☀",
        "moon" => "☾",
        _ => "•",
    }
}

#[cfg(test)]
#[path = "icon_button.test.rs"]
mod tests;
