//! The axes every component is drawn along: [`Variant`], how emphatic it is, and [`Tone`], what it means, for one that carries a colour; [`ControlSize`], how large it is, for one with metrics of its own.

use std::fmt;

use telar::{Color, Memo, Reactive, ReadSignal, RwSignal};

use crate::shared;

/// How a component carries its colour, from the most emphatic to the least. Any variant goes with any [`Tone`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, telar::PreviewArg)]
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
/// - **A control** (a button, an icon button, a field's box) is its icon with a spacing unit either side, both multiplied by [`scale`](Self::scale): 24, 32 and 40 px from small to large. The smallest is already the smallest pointer target, [`MIN_TARGET`](crate::metrics::MIN_TARGET), so nothing a pointer operates is grown past what it draws; a theme whose units would draw one below it is held at it.
/// - **A tag** moves half as far: half a control's height plus one spacing unit, 20, 24 and 28 px, and never below the smallest target where it answers the pointer, as a filter or round an action, so a small one is 24 there.
/// - **The gaps and padding** are the theme's spacing steps multiplied by [`scale`](Self::scale).
/// - **The text** steps by [`text_scale`](Self::text_scale), less than the box round it, since type below about 12 px stops reading and a line is centred in its control rather than padding it.
/// - **Corners** do not scale: a smaller control is smaller, not flatter.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, telar::PreviewArg)]
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
    /// A colour of the caller's, re-read wherever the component paints, so a theme token or a signal re-colours it live. `transparent` is transparent.
    Color(Reactive<Color>),
}

/// The names the named tones are chosen by, in the order a control lists them.
const NAMES: [&str; 6] = ["Accent", "Neutral", "Info", "Success", "Warning", "Danger"];

impl Tone {
    /// The theme's colour for this tone, or the caller's, read now.
    pub(crate) fn color(&self) -> Color {
        let theme = telar::use_theme_tokens();
        match self {
            Tone::Accent => shared::accent(),
            Tone::Neutral => shared::muted(),
            Tone::Info => theme.info(),
            Tone::Success => theme.success(),
            Tone::Warning => theme.warning(),
            Tone::Danger => theme.error(),
            Tone::Color(color) => color.get(),
        }
    }

    fn name(&self) -> Option<&'static str> {
        let at = match self {
            Tone::Accent => 0,
            Tone::Neutral => 1,
            Tone::Info => 2,
            Tone::Success => 3,
            Tone::Warning => 4,
            Tone::Danger => 5,
            Tone::Color(_) => return None,
        };
        Some(NAMES[at])
    }
}

/// A caller's colour compares by what it reads now.
impl PartialEq for Tone {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Tone::Color(a), Tone::Color(b)) => a.get() == b.get(),
            (a, b) => a.name() == b.name(),
        }
    }
}

impl fmt::Debug for Tone {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.name() {
            Some(name) => f.write_str(name),
            None => f.debug_tuple("Color").field(&self.color()).finish(),
        }
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

// The workshop chooses among the named tones; a matrix or a saved arg may still hand it a colour.
telar::__previews! {
    impl Tone {
        fn named(name: &str) -> Option<Self> {
            Some(match name {
                "Accent" => Tone::Accent,
                "Neutral" => Tone::Neutral,
                "Info" => Tone::Info,
                "Success" => Tone::Success,
                "Warning" => Tone::Warning,
                "Danger" => Tone::Danger,
                _ => return None,
            })
        }
    }

    impl telar::preview::PreviewArg for Tone {
        const CONTROL: telar::preview::ControlKind =
            telar::preview::ControlKind::Choice { variants: &NAMES };

        fn to_arg_value(&self) -> telar::preview::ArgValue {
            match self.name() {
                Some(name) => telar::preview::ArgValue::Choice(name.to_string()),
                None => telar::preview::ArgValue::Color(self.color()),
            }
        }

        fn from_arg_value(value: &telar::preview::ArgValue) -> Option<Self> {
            match value {
                telar::preview::ArgValue::Choice(name) => Tone::named(name),
                telar::preview::ArgValue::Color(color) => Some(Tone::from(*color)),
                _ => None,
            }
        }
    }
}
