//! The words a `.rsx` writes bare for a value every component shares: `size:small`, `variant:ghost`, `tone:danger`.
//!
//! A `.rsx` transpiles knowing nothing of the props it calls, so a bare lowercase word that is no binding of its file is written as the name of that word here, `telar::kw::small`, which is the value itself, [`ControlSize::Small`]. A word no axis has, or one of another axis than the prop's, is the compiler's error on the line that wrote it. A binding of the same name wins, as it does for every bare word.
//!
//! The words are one closed list, which also feeds the editor's completion, and each is its value's text form as a preview arg, in a matrix (`size:[small regular large]`), a link and stored state.

use crate::{ControlSize, Tone, Variant};

/// A type whose values are written as words: which word writes a value, and which value a word writes.
pub trait Word: Sized {
    /// The prop name every component taking this type gives it.
    const AXIS: &'static str;
    /// Every word, in the order a control lists them.
    const WORDS: &'static [&'static str];

    /// The word that writes this value, or `None` for one no word names, such as a tone's own colour.
    fn word(&self) -> Option<&'static str>;

    /// The value `word` writes, if it is a word of this axis.
    fn from_word(word: &str) -> Option<Self>;
}

telar_macros::__vocabulary! {
    size => ControlSize,
    variant => Variant,
    tone => Tone { Color },
}

#[cfg(feature = "previews")]
mod args {
    use super::Word;
    use crate::preview::{ArgValue, ControlKind, PreviewArg};
    use crate::{ControlSize, Tone, Variant};

    fn from_choice<T: Word>(value: &ArgValue) -> Option<T> {
        match value {
            ArgValue::Choice(word) => T::from_word(word),
            _ => None,
        }
    }

    macro_rules! worded {
        ($($ty:ty),*) => {$(
            impl PreviewArg for $ty {
                const CONTROL: ControlKind = ControlKind::Choice { variants: <$ty as Word>::WORDS };

                fn to_arg_value(&self) -> ArgValue {
                    ArgValue::Choice(self.word().unwrap_or_default().to_string())
                }

                fn from_arg_value(value: &ArgValue) -> Option<Self> {
                    from_choice(value)
                }
            }
        )*};
    }

    worded!(ControlSize, Variant);

    // The control chooses among the named tones; a matrix or a stored arg may still hand it a colour.
    impl PreviewArg for Tone {
        const CONTROL: ControlKind = ControlKind::Choice {
            variants: <Tone as Word>::WORDS,
        };

        fn to_arg_value(&self) -> ArgValue {
            match self {
                Tone::Color(color) => ArgValue::Color(color.get()),
                named => ArgValue::Choice(named.word().unwrap_or_default().to_string()),
            }
        }

        fn from_arg_value(value: &ArgValue) -> Option<Self> {
            match value {
                ArgValue::Color(color) => Some(Tone::from(*color)),
                value => from_choice(value),
            }
        }
    }
}

#[cfg(test)]
#[path = "kw.test.rs"]
mod tests;
