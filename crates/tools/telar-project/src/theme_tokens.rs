//! The names `ThemeTokens` answers, for the two tools that have to know them without compiling `theme-core`: the `ThemeTokens` derive, which forwards a theme's fields to them, and the transpiler, which compiles a library's `$theme.x` to one of them.
//!
//! The trait is the vocabulary and this is its mirror, kept in step by a test that reads the trait's source. Split three ways because the derive treats each group differently; everything else asks [`is_token`].

/// The tokens whose built-in is a hard-coded constant, and therefore the ones a theme that stays silent contradicts on screen: a component answering 4px next to bars the user configured to 10.
pub const REQUIRED: &[&str] = &[
    "primary",
    "on_primary",
    "radius",
    "spacing",
    "icon_size",
    "muted",
    "scrollbar",
    "ink",
    "surface",
    "surface_alt",
    "border",
    "success",
    "warning",
    "error",
    "on_error",
    "info",
    "highlight_low",
    "highlight_med",
    "highlight_high",
];

/// Tokens whose built-in follows from the required ones or is a convention every palette shares, so silence is the right answer rather than a contradiction: the radius and spacing steps and roles move with their base, the focus ring is the primary, a hairline is one pixel and a scrim is half black.
pub const DERIVED: &[&str] = &[
    "radius_sm",
    "radius_md",
    "radius_lg",
    "radius_control",
    "radius_surface",
    "radius_dialog",
    "radius_item",
    "border_width",
    "border_width_strong",
    "focus_ring",
    "focus_ring_width",
    "focus_ring_offset",
    "font_weight_strong",
    "spacing_sm",
    "spacing_md",
    "spacing_lg",
    "spacing_xl",
    "inverse_surface",
    "inverse_ink",
    "overlay",
    "thumb",
];

/// Tokens a silent theme does not contradict, because their built-in adds nothing to the screen rather than asserting a number beside one the theme chose. `root` is the theme's row at the top of the document: say nothing and the document keeps its own, which is exactly right.
pub const OPTIONAL: &[&str] = &["root"];

/// The tokens measured in logical pixels.
const LENGTHS: &[&str] = &[
    "radius",
    "radius_sm",
    "radius_md",
    "radius_lg",
    "radius_control",
    "radius_surface",
    "radius_dialog",
    "radius_item",
    "border_width",
    "border_width_strong",
    "focus_ring_width",
    "focus_ring_offset",
    "spacing",
    "spacing_sm",
    "spacing_md",
    "spacing_lg",
    "spacing_xl",
    "icon_size",
];

/// What a token's method returns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenType {
    Color,
    /// An `f32` in logical pixels.
    Length,
    /// A `u16` font weight.
    Weight,
    /// The theme's `Declared` row at the root of the document.
    Declared,
}

/// Every token, in declaration order of the three groups.
pub fn all() -> impl Iterator<Item = &'static str> {
    REQUIRED.iter().chain(DERIVED).chain(OPTIONAL).copied()
}

/// Whether `name` is a method of `ThemeTokens`.
pub fn is_token(name: &str) -> bool {
    all().any(|token| token == name)
}

/// What the token `name` returns, or `None` when it is not a token.
pub fn token_type(name: &str) -> Option<TokenType> {
    if !is_token(name) {
        return None;
    }
    Some(match name {
        "root" => TokenType::Declared,
        "font_weight_strong" => TokenType::Weight,
        _ if LENGTHS.contains(&name) => TokenType::Length,
        _ => TokenType::Color,
    })
}
