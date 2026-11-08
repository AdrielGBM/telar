//! The axes every component is drawn along, one type each for every component that has them: [`Variant`], how emphatic it is, and [`Tone`], what it means, for one that carries a colour; [`ControlSize`], how large it is, for one with metrics of its own.
//!
//! They belong to the theme's vocabulary because a tone is the theme's token of its intent and a size is read off the theme's spacing unit and icon size. Each value has one word, which a `.rsx` writes bare (`size:small`, `variant:ghost`, `tone:danger`) and which is its text form as a preview arg; the facade's `kw` module names them.

use std::fmt;

use geometry_core::Color;
use reactive_core::{Memo, Reactive, ReadSignal, RwSignal};

use crate::use_theme_tokens;

/// How a component carries its colour, from the most emphatic to the least. Any variant goes with any [`Tone`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Variant {
    /// Filled with the colour, its label in an ink that reads on it: what a view leads with.
    #[default]
    Solid,
    /// A wash of the colour behind a label in the colour: what matters without leading.
    Soft,
    /// Framed in the colour, with the label in it.
    Outline,
    /// No surface until a pointer rests on it, the label alone carrying the colour: a toolbar's action.
    Ghost,
}

/// How large a component with metrics of its own is drawn: its height, padding, gaps, inner icon and text all follow from this one choice, made where the component is written and never inherited from what holds it.
///
/// Every length comes from the theme's spacing unit and icon size, at the default theme 8 and 16:
///
/// - **A control** (a button, an icon button, a field's box) is its icon with a spacing unit either side, both multiplied by [`scale`](Self::scale): 24, 32 and 40 px from small to large. The smallest is already the smallest pointer target, so nothing a pointer operates is grown past what it draws; a theme whose units would draw one below it is held at it.
/// - **A tag** moves half as far: half a control's height plus one spacing unit, 20, 24 and 28 px, and never below the smallest target where it answers the pointer, as a filter or round an action, so a small one is 24 there.
/// - **The gaps and padding** are the theme's spacing steps multiplied by [`scale`](Self::scale).
/// - **The text** steps by [`text_scale`](Self::text_scale), less than the box round it, since type below about 12 px stops reading and a line is centred in its control rather than padding it.
/// - **Corners** do not scale: a smaller control is smaller, not flatter.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ControlSize {
    /// Compact: a control inside another, as a tag's remove button, or a dense inspector.
    Small,
    /// The size a view is laid out at.
    #[default]
    Regular,
    /// A touch target, or a control that is the point of the screen it is on.
    Large,
}

impl ControlSize {
    /// What the theme's spacing steps and icon size are multiplied by at this size.
    pub fn scale(self) -> f32 {
        match self {
            ControlSize::Small => 0.75,
            ControlSize::Regular => 1.0,
            ControlSize::Large => 1.25,
        }
    }

    /// What the text around a control is multiplied by inside it at this size.
    pub fn text_scale(self) -> f32 {
        match self {
            ControlSize::Small => 0.875,
            ControlSize::Regular => 1.0,
            ControlSize::Large => 1.125,
        }
    }
}

/// What a component's colour says: the theme's token of that intent, so a theme restyles every tone it defines, or a colour no tone names. Every variant and every state is worked out from it, with an ink that reads on its own fill in light and dark.
///
/// A colour converts into one, so `tone:` takes `$theme.brand` or `#ff8800` as well as a name.
#[derive(Clone, Default)]
pub enum Tone {
    /// The theme's primary: the application's own colour.
    #[default]
    Accent,
    /// No intent of its own, in the theme's muted grey: a count, a plain category.
    Neutral,
    /// Worth knowing, with nothing to do about it.
    Info,
    /// Done, or going well.
    Success,
    /// Worth a look before it becomes a problem.
    Warning,
    /// Failed, destructive or wrong: the theme's `error`.
    Danger,
    /// The ink of the place the component sits in, whatever colour that is: a key cap in a menu row, a tag on a dark banner. The one tone a component resolves against where it paints rather than against the theme.
    Ink,
    /// A colour of the caller's, re-read wherever the component paints, so a theme token or a signal re-colours it live. `transparent` is transparent.
    Color(Reactive<Color>),
}

impl Tone {
    /// The colour for this tone where the component paints: the theme's, the caller's, or `ink`, the ink of the place it sits in, read now and subscribing the caller to the theme.
    pub fn color(&self, ink: Color) -> Color {
        let theme = use_theme_tokens();
        match self {
            Tone::Accent => theme.primary(),
            Tone::Neutral => theme.muted(),
            Tone::Info => theme.info(),
            Tone::Success => theme.success(),
            Tone::Warning => theme.warning(),
            Tone::Danger => theme.error(),
            Tone::Ink => ink,
            Tone::Color(color) => color.get(),
        }
    }
}

/// A caller's colour compares by what it reads now.
impl PartialEq for Tone {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Tone::Color(a), Tone::Color(b)) => a.get() == b.get(),
            (a, b) => std::mem::discriminant(a) == std::mem::discriminant(b),
        }
    }
}

impl fmt::Debug for Tone {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Tone::Accent => "Accent",
            Tone::Neutral => "Neutral",
            Tone::Info => "Info",
            Tone::Success => "Success",
            Tone::Warning => "Warning",
            Tone::Danger => "Danger",
            Tone::Ink => "Ink",
            Tone::Color(color) => return f.debug_tuple("Color").field(&color.get()).finish(),
        };
        f.write_str(name)
    }
}

impl From<Color> for Tone {
    fn from(color: Color) -> Self {
        Tone::Color(Reactive::from(color))
    }
}

impl From<Reactive<Color>> for Tone {
    fn from(color: Reactive<Color>) -> Self {
        Tone::Color(color)
    }
}

impl From<RwSignal<Color>> for Tone {
    fn from(color: RwSignal<Color>) -> Self {
        Tone::Color(Reactive::from(color))
    }
}

impl From<ReadSignal<Color>> for Tone {
    fn from(color: ReadSignal<Color>) -> Self {
        Tone::Color(Reactive::from(color))
    }
}

impl From<Memo<Color>> for Tone {
    fn from(color: Memo<Color>) -> Self {
        Tone::Color(Reactive::from(color))
    }
}

#[cfg(test)]
#[path = "appearance.test.rs"]
mod tests;
