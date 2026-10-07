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
    "info",
    "highlight_low",
    "highlight_med",
    "highlight_high",
];

/// `radius_sm`/`radius_md`/`radius_lg` derive from `radius`, so silence is the right answer rather than a contradiction — a theme moves the base and the steps follow.
pub const DERIVED: &[&str] = &[
    "radius_sm",
    "radius_md",
    "radius_lg",
    "spacing_sm",
    "spacing_md",
    "spacing_lg",
    "spacing_xl",
];

/// Tokens a silent theme does not contradict, because their built-in adds nothing to the screen rather than asserting a number beside one the theme chose. `root` is the theme's row at the top of the document: say nothing and the document keeps its own, which is exactly right.
pub const OPTIONAL: &[&str] = &["root"];

/// Every token, in declaration order of the three groups.
pub fn all() -> impl Iterator<Item = &'static str> {
    REQUIRED.iter().chain(DERIVED).chain(OPTIONAL).copied()
}

/// Whether `name` is a method of `ThemeTokens`.
pub fn is_token(name: &str) -> bool {
    all().any(|token| token == name)
}
