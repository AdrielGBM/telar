//! [`icon_button`]: a square button drawn from an icon, named by a tooltip and by a screen reader.

use std::rc::Rc;

use telar::{
    Accessible, AlignItems, Border, BorderRadius, Children, Color, JustifyContent, LayoutError,
    LayoutItem, LayoutStyle, Props, Reactive, RectStyle, ShapeStyle, Slots, StyledContainer,
    box_item, focus::Role,
};

use crate::shared;
use crate::toolbar::ToolbarContext;
use crate::{TooltipProps, tooltip};

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
    /// The key binding that does the same thing, shown in the tooltip: `"Ctrl+K"`. Empty means none.
    #[props(into, default)]
    pub shortcut: Reactive<String>,
    /// Makes the button a toggle that reads as pressed while this is true. Unset, it is a plain button and says nothing about being pressed.
    #[props(some, into, default)]
    pub pressed: Option<Reactive<bool>>,
    #[props(into, default)]
    pub disabled: Reactive<bool>,
    #[props(default = Rc::new(|| {}))]
    pub on_press: Rc<dyn Fn()>,
}

fn pad() -> f32 {
    shared::spacing() * 0.75
}

fn shell() -> LayoutStyle {
    LayoutStyle::new()
        .flex_row()
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER)
        .padding_all(pad())
}

fn paint(pressed: bool, hovered: bool, cursor: bool) -> RectStyle {
    let wash = match (pressed, hovered) {
        (true, true) => 0.28,
        (true, false) => 0.2,
        (false, true) => 0.1,
        (false, false) => 0.0,
    };
    let rect = RectStyle::default().with_radius(BorderRadius::all(shared::radius()));
    let rect = if wash > 0.0 {
        rect.with_fill(shared::accent().with_alpha(wash))
    } else {
        rect
    };
    if cursor {
        rect.with_border(Border::uniform(shared::accent(), 2.0))
    } else {
        rect
    }
}

/// A square button drawn from an icon, with a tooltip and an optional pressed state.
///
/// Inside a [`toolbar`](crate::toolbar) it is an item of that toolbar and not a tab stop of its own.
pub fn icon_button(
    props: IconButtonProps,
    _children: Children,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let IconButtonProps {
        icon,
        label,
        shortcut,
        pressed,
        disabled,
        on_press,
    } = props;
    let toolbar = telar::use_context::<ToolbarContext>();

    let is_pressed = {
        let pressed = pressed.clone();
        move || pressed.as_ref().is_some_and(|pressed| pressed.get())
    };
    let is_disabled = {
        let disabled = disabled.clone();
        move || disabled.get()
    };

    let activate: Rc<dyn Fn()> = Rc::new(move || on_press());
    let index = toolbar.as_ref().map(|toolbar| {
        let disabled = disabled.clone();
        toolbar.claim(move || !disabled.get(), activate.clone())
    });
    let cursor = {
        let toolbar = toolbar.clone();
        move || match (&toolbar, index) {
            (Some(toolbar), Some(index)) => toolbar.shows_cursor_at(index),
            _ => false,
        }
    };

    let picture = picture(icon, disabled.clone())?;
    let (base_pressed, base_cursor) = (is_pressed.clone(), cursor.clone());
    let (hover_pressed, hover_cursor) = (is_pressed.clone(), cursor);
    let press = {
        let toolbar = toolbar.clone();
        move || {
            if let (Some(toolbar), Some(index)) = (&toolbar, index) {
                toolbar.select(index);
            }
            activate();
        }
    };
    let name = label.clone();
    let button = StyledContainer::new(
        shell(),
        move |_| paint(base_pressed(), false, base_cursor()),
        vec![picture],
    )?
    .styled_by(shell)
    .hover_style(move |_| paint(hover_pressed(), true, hover_cursor()))
    .disabled(is_disabled)
    .disabled_style(|_| RectStyle::default().with_radius(BorderRadius::all(shared::radius())))
    .on_press(press)
    .a11y_label(move || name.get());

    let button = match index {
        Some(_) => button.presented(Role::Button),
        None => button.control(Role::Button),
    };
    let button = if pressed.is_some() {
        button.toggled(is_pressed)
    } else {
        button
    };
    if let (Some(toolbar), Some(index)) = (&toolbar, index) {
        toolbar.present(index, button.focus_id());
    }

    let mut slots = Slots::new();
    slots.push(None, box_item(button));
    tooltip(
        TooltipProps::props().text(label).shortcut(shortcut).build(),
        Children::from(slots),
    )
}

fn ink(disabled: Reactive<bool>) -> Reactive<Color> {
    Reactive::of(move || {
        if disabled.get() {
            shared::muted().with_alpha(0.5)
        } else {
            Color::TRANSPARENT
        }
    })
}

#[cfg(feature = "icons")]
fn picture(
    icon: telar_icons::IconName,
    disabled: Reactive<bool>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    use telar_icons::{IconProps, icon as draw};

    draw(
        IconProps::props()
            .name(icon)
            .size(shared::icon_size())
            .color(ink(disabled))
            .build(),
        Children::default(),
    )
}

#[cfg(not(feature = "icons"))]
fn picture(
    icon: Reactive<String>,
    disabled: Reactive<bool>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    use telar::{Text, TextWrap};

    let color = ink(disabled);
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
    let frame = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER)
            .width(size)
            .height(size)
            .flex_shrink(0.0),
        |_| RectStyle::default(),
        vec![box_item(glyph)],
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
