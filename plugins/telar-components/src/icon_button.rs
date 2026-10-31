//! [`icon_button`]: a square button drawn from an icon, named by a tooltip and by a screen reader.

use std::rc::Rc;

use telar::{
    Accessible, AlignItems, BorderRadius, Color, JustifyContent, LayoutError, LayoutItem,
    LayoutStyle, Props, Reactive, RectStyle, ShapeStyle, StyledContainer, SurfaceAmendment,
    amend_surface, box_item, focus::Role,
};

use crate::shared;
use crate::toolbar::ToolbarContext;

/// A button that is only a picture, so its name is not optional: `label` is what the tooltip says and what a screen reader announces.
#[derive(Props)]
pub struct IconButtonProps {
    /// The icon: one baked into the calling crate, `telar_icons::icon!("lucide:search")`, or an id, `"lucide:search"`, which only a runtime source installed through `telar-icons` can draw.
    #[cfg(feature = "icons")]
    #[props(into)]
    pub icon: telar_icons::IconName,
    /// The icon's id, `set:name`: `"lucide:search"`, drawn as a Unicode glyph read from its name. Under the `icons` feature it is drawn by `telar-icons` instead.
    #[cfg(not(feature = "icons"))]
    #[props(into)]
    pub icon: Reactive<String>,
    /// The button's name.
    #[props(into)]
    pub label: Reactive<String>,
    /// The key binding that does the same thing, written portably (`"Mod+K"`) and shown in the tooltip as the platform writes it. Empty means none.
    #[props(into, default)]
    pub shortcut: Reactive<String>,
    /// Makes the button a toggle that reads as pressed while this is true. Unset, it is a plain button and says nothing about being pressed.
    #[props(some, into, default)]
    pub pressed: Option<Reactive<bool>>,
    #[props(into, default)]
    pub disabled: Reactive<bool>,
    #[props(default = Rc::new(|| {}))]
    pub on_press: Rc<dyn Fn()>,
    /// Amends the paint of the button's square — its **principal surface** — at rest, hovered, pressed and disabled. See [`telar::SurfaceStyle`] for when a theme token or [`IconButtonTokens`] is the right instrument instead.
    #[props(some, default)]
    pub style: Option<SurfaceAmendment>,
}

/// An icon button's own tokens, which an application's theme supplies through a `#[theme(extension)]` field.
///
/// Every field left `None` follows the theme role named beside it, read when the button paints, so a theme that moves the role still moves every icon button it has not given a value of its own.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct IconButtonTokens {
    /// Corner radius. `None` is the theme's `radius_control`.
    pub radius: Option<f32>,
}

impl IconButtonTokens {
    /// The tokens of the theme in force, subscribing the caller to it.
    pub fn current() -> Self {
        telar::use_theme_extension::<Self>()
    }

    pub fn radius(&self) -> f32 {
        self.radius.unwrap_or_else(shared::radius_control)
    }
}

fn shell() -> LayoutStyle {
    LayoutStyle::row()
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER)
        .padding_all(shared::icon_pad())
        .min_width(shared::control_height())
        .min_height(shared::control_height())
}

/// Which of its paints the square is showing.
#[derive(Clone, Copy)]
enum State {
    Rest,
    Hovered,
    Pressed,
}

/// The square's paint before the caller's amendment: the theme's washes for a pointer resting on it, for a toggle held on and for a press, and the theme's focus ring where a toolbar's cursor stands.
fn paint(state: State, toggled: bool, cursor: bool) -> RectStyle {
    let rect =
        RectStyle::default().with_radius(BorderRadius::all(IconButtonTokens::current().radius()));
    let wash = match (state, toggled) {
        (State::Rest, false) => None,
        (State::Hovered, false) => Some(shared::hover_wash()),
        (State::Rest, true) => Some(shared::held_wash()),
        (State::Hovered, true) | (State::Pressed, _) => Some(shared::pressed_wash()),
    };
    let rect = match wash {
        Some(wash) => rect.with_fill(wash),
        None => rect,
    };
    if cursor {
        rect.with_border(shared::focus_ring())
    } else {
        rect
    }
}

/// A square button drawn from an icon, with a tooltip and an optional pressed state.
///
/// Inside a [`toolbar`](crate::toolbar) it is an item of that toolbar and not a tab stop of its own.
pub fn icon_button(props: IconButtonProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let IconButtonProps {
        icon,
        label,
        shortcut,
        pressed,
        disabled,
        on_press,
        style,
    } = props;
    let toolbar = telar::use_context::<ToolbarContext>();

    let activate: Rc<dyn Fn()> = Rc::new(move || on_press());
    let index = toolbar.as_ref().map(|toolbar| {
        let disabled = disabled.clone();
        toolbar.claim(move || !disabled.get(), activate.clone())
    });
    let square = Square {
        icon,
        label: label.clone(),
        shortcut: shortcut.clone(),
        pressed,
        disabled,
        activate,
        style,
        toolbar,
        index,
    };

    shared::hinted(move || square.build(), label, shortcut)
}

/// Everything the square under an icon button's tooltip is built from, so the tooltip can build it wherever and however often it places it.
#[derive(Clone)]
struct Square {
    icon: IconSource,
    label: Reactive<String>,
    shortcut: Reactive<String>,
    pressed: Option<Reactive<bool>>,
    disabled: Reactive<bool>,
    activate: Rc<dyn Fn()>,
    style: Option<SurfaceAmendment>,
    toolbar: Option<ToolbarContext>,
    index: Option<usize>,
}

impl Square {
    fn build(&self) -> Result<Box<dyn LayoutItem>, LayoutError> {
        let Square {
            icon,
            label,
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
            let toolbar = toolbar.clone();
            move || match (&toolbar, index) {
                (Some(toolbar), Some(index)) => toolbar.shows_cursor_at(index),
                _ => false,
            }
        };

        let picture = picture(icon, ink(disabled))?;
        let surface = {
            let is_pressed = is_pressed.clone();
            Rc::new(move |state: State, at_cursor: bool| {
                amend_surface(paint(state, is_pressed(), at_cursor), &style)
            })
        };
        let (rest, hover, pressing, idle) =
            (surface.clone(), surface.clone(), surface.clone(), surface);
        let (rest_cursor, hover_cursor, press_cursor) = (cursor.clone(), cursor.clone(), cursor);
        let press = {
            let toolbar = toolbar.clone();
            move || {
                if let (Some(toolbar), Some(index)) = (&toolbar, index) {
                    toolbar.select(index);
                }
                activate();
            }
        };
        let button = StyledContainer::new(
            shell(),
            move |_| rest(State::Rest, rest_cursor()),
            vec![picture],
        )?
        .styled_by(shell)
        .hover_style(move |_| hover(State::Hovered, hover_cursor()))
        .active_style(move |_| pressing(State::Pressed, press_cursor()))
        .disabled(is_disabled)
        .disabled_style(move |_| idle(State::Rest, false))
        .on_press(press)
        .a11y_label(move || label.get());

        let button = match index {
            Some(_) => button.presented(Role::Button),
            None => button.control(Role::Button),
        };
        let button = button.keyboard_shortcut(move || shortcut.get());
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

fn ink(disabled: Reactive<bool>) -> Reactive<Color> {
    Reactive::of(move || {
        if disabled.get() {
            shared::muted()
        } else {
            Color::TRANSPARENT
        }
    })
}

/// What names an icon: an [`IconName`](telar_icons::IconName) under the `icons` feature, and an id drawn as a Unicode glyph without it.
#[cfg(feature = "icons")]
pub(crate) type IconSource = telar_icons::IconName;
#[cfg(not(feature = "icons"))]
pub(crate) type IconSource = Reactive<String>;

/// `icon` at the size a control draws one, in `color`, or in the text around it while that is transparent.
#[cfg(feature = "icons")]
pub(crate) fn picture(
    icon: IconSource,
    color: Reactive<Color>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    use telar_icons::{IconProps, icon as draw};

    draw(
        IconProps::props()
            .name(icon)
            .size(shared::icon_size())
            .color(color)
            .build(),
    )
}

#[cfg(not(feature = "icons"))]
pub(crate) fn picture(
    icon: IconSource,
    color: Reactive<Color>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    use telar::{Text, TextWrap};

    let size = shared::icon_size();
    let glyph = Text::declaring(
        move || glyph_for(&icon.get()).to_string(),
        LayoutStyle::new(),
        move |t| {
            let text = shared::control_text(t, 1.0).with_text_wrap(TextWrap::NoWrap);
            match color.get() {
                Color::TRANSPARENT => text,
                dimmed => text.with_color(dimmed),
            }
        },
    )?;
    let frame = Container::new(
        LayoutStyle::row()
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER)
            .width(size)
            .height(size)
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
        "search" | "magnifying-glass" => "⌕",
        "close" | "x" | "cross" => "✕",
        "plus" | "add" => "+",
        "minus" | "remove" => "−",
        "check" | "done" => "✓",
        "settings" | "cog" | "gear" => "⚙",
        "menu" => "☰",
        "play" => "▶",
        "pause" => "⏸",
        "stop" => "■",
        "refresh" | "rotate-cw" | "reload" => "↻",
        "undo" | "rotate-ccw" => "↺",
        "chevron-left" | "arrow-left" => "←",
        "chevron-right" | "arrow-right" => "→",
        "chevron-up" | "arrow-up" => "↑",
        "chevron-down" | "arrow-down" => "↓",
        "star" => "★",
        "info" => "ⓘ",
        "copy" => "⧉",
        "grid" => "▦",
        "sun" => "☀",
        "moon" => "☾",
        _ => "•",
    }
}

#[cfg(test)]
#[path = "icon_button_test.rs"]
mod tests;
