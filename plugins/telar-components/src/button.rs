//! [`button`]: the pressable control, in its solid, outline, soft and ghost variants.

use std::rc::Rc;

use telar::{
    AlignItems, Border, BorderRadius, Color, JustifyContent, LayoutError, LayoutItem, LayoutStyle,
    Props, Reactive, RectStyle, ShapeStyle, StyledContainer, SurfaceAmendment, Text, TextWrap,
    amend_surface, box_item, children, focus::Role,
};

use crate::shared;

/// How a button carries its colour, from the most emphatic to the least.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, telar::PreviewArg)]
pub enum ButtonVariant {
    /// Filled with the colour, the label in an ink that reads on it: the action a view leads with.
    #[default]
    Solid,
    /// Framed in the colour with the label in it.
    Outline,
    /// A wash of the colour behind a label in the colour: an action that matters without leading.
    Soft,
    /// No surface until a pointer rests on it, and the label in the ink around it: a toolbar's action.
    Ghost,
}

/// A button's own tokens, which an application's theme supplies through a `#[theme(extension)]` field.
///
/// Every field left `None` follows the theme role named beside it, read when the button paints, so a theme that moves the role still moves every button it has not given a value of its own.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ButtonTokens {
    /// Corner radius. `None` is the theme's `radius_control`.
    pub radius: Option<f32>,
    /// Space between the edge and the label at the regular control size, scaled with the control size in force. `None` is [`metrics::control_pad_x`](crate::metrics::control_pad_x).
    pub pad_x: Option<f32>,
    /// Width of the outline variant's frame. `None` is the theme's `border_width_strong`.
    pub border_width: Option<f32>,
}

impl ButtonTokens {
    /// The tokens of the theme in force, subscribing the caller to it.
    pub fn current() -> Self {
        telar::use_theme_extension::<Self>()
    }

    pub fn radius(&self) -> f32 {
        self.radius.unwrap_or_else(shared::radius_control)
    }

    pub fn pad_x(&self) -> f32 {
        self.pad_x
            .map_or_else(shared::control_pad_x, |pad| pad * telar::control_scale())
    }

    pub fn border_width(&self) -> f32 {
        self.border_width
            .unwrap_or_else(shared::border_width_strong)
    }
}

/// The label never wraps: a button names one action, and two lines of it without vertical padding would crowd the edge, so the layout around it gives way instead. A row so the label's measured width sets the box's main-axis size (a column would collapse the cross axis: `Text::new` sets `align_self_stretch`, which fights content-sizing and renders 0-wide). No vertical padding: the line is centred in the shared control height, so a button stands level with a field beside it.
fn shell() -> LayoutStyle {
    LayoutStyle::row()
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER)
        .padding_horizontal(ButtonTokens::current().pad_x())
        .min_height(shared::control_height())
}

/// A labelled, pressable button. This is the high-level convenience over the primitives (`box` + `on_press` + `hover` + a centred `text`); it lives in `telar-components`, not the kernel, so an app can drop it or ship its own.
#[derive(Props)]
pub struct ButtonProps {
    /// The text on the button.
    #[props(into, default)]
    pub label: Reactive<String>,
    /// How the button carries its colour.
    #[props(default)]
    pub variant: ButtonVariant,
    /// The colour the variant is drawn in. `Color::TRANSPARENT` (the default) is the theme's primary. A closure, so a theme token re-reads on a switch.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub color: Reactive<Color>,
    /// Runs when the button is pressed.
    #[props(default = Rc::new(|| {}))]
    pub on_press: Rc<dyn Fn()>,
    /// Amends the paint of the button's box — its **principal surface** — at rest, hovered and pressed. See [`telar::SurfaceStyle`] for when a theme token or [`ButtonTokens`] is the right instrument instead.
    #[props(some, default)]
    pub style: Option<SurfaceAmendment>,
}

/// Which of its paints the box is showing.
#[derive(Clone, Copy)]
enum State {
    Rest,
    Hovered,
    Pressed,
}

/// The pressable control, in its solid, outline, soft and ghost variants.
pub fn button(props: ButtonProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let ButtonProps {
        label,
        variant,
        color,
        on_press,
        style,
    } = props;

    let label_tone = color.clone();
    let label_widget = Text::declaring(
        move || label.get(),
        LayoutStyle::new(),
        move |t| {
            let text = shared::control_text(t, 1.0).with_text_wrap(TextWrap::NoWrap);
            match label_color(variant, tone(&label_tone)) {
                Some(color) => text.with_color(color),
                None => text,
            }
        },
    )?;

    let paint =
        Rc::new(move |state: State| amend_surface(surface(variant, tone(&color), state), &style));
    let (rest, hover, press) = (paint.clone(), paint.clone(), paint);
    let container = StyledContainer::new(
        shell(),
        move |_r| rest(State::Rest),
        children![label_widget],
    )?
    .styled_by(shell)
    .hover_style(move |_r| hover(State::Hovered))
    .active_style(move |_r| press(State::Pressed))
    .control(Role::Button)
    .on_press(move || on_press());
    Ok(box_item(container))
}

/// The colour the variant is drawn in, `None` where the caller left it to the theme.
fn tone(color: &Reactive<Color>) -> Option<Color> {
    let color = color.get();
    (color != Color::TRANSPARENT).then_some(color)
}

/// The box's paint in `state`, before the caller's amendment. A filled box darkens under the pointer; every other variant lays the theme's washes over what it shows at rest, so its label reads the same in every state.
fn surface(variant: ButtonVariant, tone: Option<Color>, state: State) -> RectStyle {
    let tokens = ButtonTokens::current();
    let rect = RectStyle::default().with_radius(BorderRadius::all(tokens.radius()));
    let color = tone.unwrap_or_else(shared::accent);
    match variant {
        ButtonVariant::Solid => rect.with_fill(match state {
            State::Rest => color,
            State::Hovered => shared::hovered_fill(color),
            State::Pressed => shared::pressed_fill(color),
        }),
        ButtonVariant::Outline => {
            let framed = rect.with_border(Border::uniform(color, tokens.border_width()));
            match state {
                State::Rest => framed,
                State::Hovered => framed.with_fill(washed(color, shared::hover_wash())),
                State::Pressed => framed.with_fill(washed(color, shared::pressed_wash())),
            }
        }
        ButtonVariant::Soft => rect.with_fill(match state {
            State::Rest => washed(color, shared::held_wash()),
            State::Hovered => washed(color, shared::pressed_wash()),
            State::Pressed => shared::pressed_fill(washed(color, shared::pressed_wash())),
        }),
        ButtonVariant::Ghost => match state {
            State::Rest => rect,
            State::Hovered => rect.with_fill(shared::hover_wash()),
            State::Pressed => rect.with_fill(shared::pressed_wash()),
        },
    }
}

/// `color` at the strength of one of the theme's washes, so a tinted button is as light or as strong as the theme's hover and selection.
fn washed(color: Color, wash: Color) -> Color {
    color.with_alpha(wash.a)
}

/// The label's colour: an ink that reads on a filled box, the colour itself in an outline, the colour taken just far enough toward an ink to read on the strongest wash of itself a soft button shows, and on a ghost the colour only when the caller named one.
///
/// `None` means "whatever the page says". A ghost button paints no surface of its own, so its label sits on the page beside ordinary text — naming the theme's `ink` here made it the one word in a region that had declared its colour that came out in the theme's.
fn label_color(variant: ButtonVariant, tone: Option<Color>) -> Option<Color> {
    match variant {
        ButtonVariant::Solid => Some(tone.map_or_else(shared::on_accent, shared::ink_on)),
        ButtonVariant::Outline => Some(tone.unwrap_or_else(shared::accent)),
        ButtonVariant::Soft => {
            let color = tone.unwrap_or_else(shared::accent);
            let fill = shared::over(washed(color, shared::pressed_wash()), shared::surface());
            Some(shared::readable_on(color, fill))
        }
        ButtonVariant::Ghost => tone,
    }
}

#[cfg(test)]
#[path = "button_test.rs"]
mod tests;
