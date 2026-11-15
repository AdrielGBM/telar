//! The pairs the catalogue draws one colour token on another, and the contrast each owes: the table a theme is checked against, at the regular contrast and at the high contrast a person can ask for.

use std::fmt;
use std::rc::Rc;

use geometry_core::{BODY_TEXT_CONTRAST, Color, ENHANCED_TEXT_CONTRAST, NON_TEXT_CONTRAST};

use crate::ThemeTokens;
use crate::high_contrast::{HIGH_CONTRAST_BOUNDARY, HighContrast};

/// A colour token of [`ThemeTokens`] that the contrast table names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ColorToken {
    Ink,
    Muted,
    Primary,
    OnPrimary,
    Success,
    Warning,
    Info,
    Error,
    OnError,
    InverseInk,
    InverseSurface,
    ControlBorder,
    FocusRing,
    Surface,
    SurfaceAlt,
}

impl ColorToken {
    /// The name of the token's method on [`ThemeTokens`], which is also the field a derived theme sets it with.
    pub fn name(self) -> &'static str {
        match self {
            ColorToken::Ink => "ink",
            ColorToken::Muted => "muted",
            ColorToken::Primary => "primary",
            ColorToken::OnPrimary => "on_primary",
            ColorToken::Success => "success",
            ColorToken::Warning => "warning",
            ColorToken::Info => "info",
            ColorToken::Error => "error",
            ColorToken::OnError => "on_error",
            ColorToken::InverseInk => "inverse_ink",
            ColorToken::InverseSurface => "inverse_surface",
            ColorToken::ControlBorder => "control_border",
            ColorToken::FocusRing => "focus_ring",
            ColorToken::Surface => "surface",
            ColorToken::SurfaceAlt => "surface_alt",
        }
    }

    /// The token as `tokens` answer it, laid over their `surface`, which is how every one of them is seen.
    pub fn of(self, tokens: &dyn ThemeTokens) -> Color {
        let color = match self {
            ColorToken::Ink => tokens.ink(),
            ColorToken::Muted => tokens.muted(),
            ColorToken::Primary => tokens.primary(),
            ColorToken::OnPrimary => tokens.on_primary(),
            ColorToken::Success => tokens.success(),
            ColorToken::Warning => tokens.warning(),
            ColorToken::Info => tokens.info(),
            ColorToken::Error => tokens.error(),
            ColorToken::OnError => tokens.on_error(),
            ColorToken::InverseInk => tokens.inverse_ink(),
            ColorToken::InverseSurface => tokens.inverse_surface(),
            ColorToken::ControlBorder => tokens.control_border(),
            ColorToken::FocusRing => tokens.focus_ring(),
            ColorToken::Surface => tokens.surface(),
            ColorToken::SurfaceAlt => tokens.surface_alt(),
        };
        color.over(tokens.surface())
    }
}

/// How much contrast a token set is held to: the regular WCAG AA minima, or the higher ones owed to a person who asked the system or the app for more contrast.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ContrastLevel {
    Standard,
    High,
}

/// What a component draws a token as, which decides the contrast it owes what it is drawn on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ContrastUse {
    /// Text, or a glyph read as text.
    Text,
    /// What tells a control from what it sits on: its frame at rest, or the keyboard's ring.
    ControlBoundary,
}

impl ContrastUse {
    /// The least contrast ratio this use owes at `level`: 4.5:1 for text and 3:1 for a boundary at the standard level, 7:1 and 4.5:1 at the high one.
    pub fn minimum(self, level: ContrastLevel) -> f32 {
        match (self, level) {
            (ContrastUse::Text, ContrastLevel::Standard) => BODY_TEXT_CONTRAST,
            (ContrastUse::Text, ContrastLevel::High) => ENHANCED_TEXT_CONTRAST,
            (ContrastUse::ControlBoundary, ContrastLevel::Standard) => NON_TEXT_CONTRAST,
            (ContrastUse::ControlBoundary, ContrastLevel::High) => HIGH_CONTRAST_BOUNDARY,
        }
    }
}

/// One token the catalogue draws on another, and what it draws it as.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ContrastPair {
    pub drawn: ColorToken,
    pub on: ColorToken,
    pub used: ContrastUse,
}

const fn pair(drawn: ColorToken, on: ColorToken, used: ContrastUse) -> ContrastPair {
    ContrastPair { drawn, on, used }
}

/// Every pair the catalogue draws one token on another. `surface_alt` is seen laid over `surface`.
pub const CONTRAST_PAIRS: &[ContrastPair] = {
    use ColorToken::*;
    use ContrastUse::*;
    &[
        pair(Ink, Surface, Text),
        pair(Ink, SurfaceAlt, Text),
        pair(Muted, Surface, Text),
        pair(Muted, SurfaceAlt, Text),
        pair(Primary, Surface, Text),
        pair(Primary, SurfaceAlt, Text),
        pair(Success, Surface, Text),
        pair(Success, SurfaceAlt, Text),
        pair(Warning, Surface, Text),
        pair(Warning, SurfaceAlt, Text),
        pair(Info, Surface, Text),
        pair(Info, SurfaceAlt, Text),
        pair(Error, Surface, Text),
        pair(Error, SurfaceAlt, Text),
        pair(OnPrimary, Primary, Text),
        pair(OnError, Error, Text),
        pair(InverseInk, InverseSurface, Text),
        pair(ControlBorder, Surface, ControlBoundary),
        pair(ControlBorder, SurfaceAlt, ControlBoundary),
        pair(FocusRing, Surface, ControlBoundary),
    ]
};

/// A pair of the table that falls short of what it owes at a level.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContrastShortfall {
    pub pair: ContrastPair,
    pub level: ContrastLevel,
    /// The ratio the pair reaches.
    pub ratio: f32,
}

impl fmt::Display for ContrastShortfall {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ContrastPair { drawn, on, used } = self.pair;
        write!(
            f,
            "{level:?} contrast: {drawn} on {on} is {ratio:.2}:1, {used:?} needs {minimum}:1",
            level = self.level,
            drawn = drawn.name(),
            on = on.name(),
            ratio = self.ratio,
            minimum = used.minimum(self.level),
        )
    }
}

/// Every pair of [`CONTRAST_PAIRS`] that `theme` falls short on: as the theme answers it at [`ContrastLevel::Standard`], and as the high-contrast set derived from it answers it at [`ContrastLevel::High`]. Empty for a theme the catalogue reads well in.
///
/// A token that follows the light/dark mode answers for the mode in force where this is called, so a theme with both is checked once in each.
pub fn contrast_shortfalls<T: ThemeTokens>(theme: T) -> Vec<ContrastShortfall> {
    let standard: Rc<dyn ThemeTokens> = Rc::new(theme);
    let high = HighContrast(Rc::clone(&standard));
    let mut shortfalls = shortfalls_at(&*standard, ContrastLevel::Standard);
    shortfalls.extend(shortfalls_at(&high, ContrastLevel::High));
    shortfalls
}

fn shortfalls_at(tokens: &dyn ThemeTokens, level: ContrastLevel) -> Vec<ContrastShortfall> {
    CONTRAST_PAIRS
        .iter()
        .filter_map(|&pair| {
            let ratio = pair.drawn.of(tokens).contrast_ratio(pair.on.of(tokens));
            (ratio < pair.used.minimum(level)).then_some(ContrastShortfall { pair, level, ratio })
        })
        .collect()
}

#[cfg(test)]
#[path = "contrast.test.rs"]
mod tests;
